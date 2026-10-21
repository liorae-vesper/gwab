//! End-to-end request tests against a small local HTTP echo server.
#![allow(clippy::expect_used)]

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-workspaces")
            .join(format!("{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("test workspace should be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct HttpTestServer {
    address: SocketAddr,
    requests: Receiver<String>,
    worker: JoinHandle<()>,
}

impl HttpTestServer {
    fn start(request_count: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
        listener
            .set_nonblocking(true)
            .expect("test server should be nonblocking");
        let address = listener
            .local_addr()
            .expect("test server should have an address");
        let (sender, requests) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut handled = 0;
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while handled < request_count && std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        let request = read_request(&mut stream);
                        let response = make_response(&request);
                        let _ = stream.write_all(response.as_bytes());
                        let _ = sender.send(request);
                        handled += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            requests,
            worker,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    fn finish(self) -> Vec<String> {
        self.worker.join().expect("test server should finish");
        self.requests.try_iter().collect()
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    while let Ok(size) = stream.read(&mut buffer) {
        if size == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..size]);
        if let Some(header_end) = find_header_end(&bytes) {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn make_response(request: &str) -> String {
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let cookie = request
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("cookie"))
        .map_or("", |(_, value)| value.trim());
    let status = path
        .strip_prefix("/status/")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(200);
    let (extra_headers, body) = if path == "/set" {
        (
            concat!(
                "Set-Cookie: session=abc; Path=/; HttpOnly; SameSite=Lax\r\n",
                "Set-Cookie: private=only; Path=/private\r\n",
                "Set-Cookie: expired=never; Path=/; Max-Age=0\r\n"
            ),
            "cookies set".to_owned(),
        )
    } else if path == "/response" {
        ("", "response payload".to_owned())
    } else {
        ("", cookie.to_owned())
    };
    format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
        body.len(),
        extra_headers,
        body
    )
}

#[test]
fn quiet_mode_silently_maps_success_range_to_zero_and_other_statuses_to_one() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(4);

    for (path, expected_code) in [
        ("/status/200", 0),
        ("/status/299", 0),
        ("/status/300", 1),
        ("/status/404", 1),
    ] {
        let output = if path == "/status/299" {
            run_gwab(
                &directory,
                &["get", &server.url(path), "--quiet", "--verbose"],
            )
        } else {
            run_gwab(&directory, &["get", &server.url(path), "--quiet"])
        };
        assert_eq!(output.status.code(), Some(expected_code), "{path}");
        assert!(
            output.stdout.is_empty(),
            "quiet mode wrote stdout for {path}"
        );
        assert!(
            output.stderr.is_empty(),
            "quiet mode wrote stderr for {path}"
        );
    }

    assert_eq!(server.finish().len(), 4);
}

#[test]
fn quiet_mode_silences_request_errors() {
    let directory = TestDirectory::new();
    let output = run_gwab(&directory, &["get", "/posts", "--quiet"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert_eq!(output.stderr, Vec::<u8>::new());
}

#[test]
fn missing_variables_fail_with_a_hint_when_stdin_is_not_a_terminal() {
    let directory = TestDirectory::new();
    let output = run_gwab(&directory, &["get", "http://127.0.0.1:1/users/{{user_id}}"]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing variable: user_id"));
    assert!(stderr.contains("--var"));
}

#[test]
fn quiet_mode_does_not_prompt_or_print_for_missing_variables() {
    let directory = TestDirectory::new();
    let output = run_gwab(
        &directory,
        &["get", "http://127.0.0.1:1/users/{{user_id}}", "--quiet"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert_eq!(output.stderr, Vec::<u8>::new());
}

#[test]
fn saved_requests_round_trip_cli_options_and_replay_from_the_current_directory() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(2);
    let url = server.url("/response");

    let saved = run_gwab(
        &directory,
        &[
            "get",
            &url,
            "--add-header",
            "X-Saved: yes",
            "--var",
            "tenant=7",
            "--output",
            "body",
            "--description",
            "List users",
            "--save",
            "users",
        ],
    );
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );

    let request_file = directory.path().join("gwab-requests.toml");
    let contents = fs::read_to_string(&request_file).expect("saved request file should exist");
    let requests: toml::Table = toml::from_str(&contents).expect("saved requests should be TOML");
    let entry = requests
        .get("get users")
        .and_then(toml::Value::as_table)
        .expect("request should be stored under its method and name");
    assert_eq!(
        entry.get("url").and_then(toml::Value::as_str),
        Some(url.as_str())
    );
    assert!(entry.contains_key("add-header"));
    assert!(entry.contains_key("var"));
    assert_eq!(
        entry.get("description").and_then(toml::Value::as_str),
        Some("List users")
    );
    assert_eq!(
        entry.get("output").and_then(toml::Value::as_str),
        Some("body")
    );

    let replayed = run_gwab(&directory, &["run", "get", "users"]);
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /response "));
    assert!(requests[1].contains("x-saved: yes"));
    assert_eq!(replayed.stdout, b"response payload");
}

#[test]
fn saving_a_piped_body_keeps_it_for_replay() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(2);
    let body = br#"{"from":"saved stdin"}"#;
    let url = server.url("/inspect");

    let saved = run_gwab_with_stdin(&directory, &["post", &url, "--save", "piped-body"], body);
    assert!(saved.status.success());
    let replayed = run_gwab(&directory, &["run", "post", "piped-body"]);
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );

    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    let body = std::str::from_utf8(body).expect("JSON body should be UTF-8");
    assert!(requests[0].ends_with(body));
    assert!(requests[1].ends_with(body));
}

#[test]
fn replaying_an_unknown_saved_request_returns_a_clear_error() {
    let directory = TestDirectory::new();

    let output = run_gwab(&directory, &["run", "get", "missing"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("saved request not found"));
}

#[test]
fn saved_request_picker_requires_a_terminal_when_requests_are_available() {
    let directory = TestDirectory::new();
    fs::write(
        directory.path().join("gwab-requests.toml"),
        "[\"get users\"]\nurl = \"/users\"\ndescription = \"List users\"\n",
    )
    .expect("saved request should be written");

    let output = run_gwab(&directory, &["run"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
}

#[test]
fn piped_stdin_is_sent_as_the_request_body() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(1);
    let body = br#"{"source":"stdin"}"#;

    let output = run_gwab_with_stdin(&directory, &["post", &server.url("/inspect")], body);
    assert!(output.status.success());
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].ends_with(std::str::from_utf8(body).expect("JSON is UTF-8")));
}

#[test]
fn explicit_body_takes_precedence_over_piped_stdin() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(1);

    let output = run_gwab_with_stdin(
        &directory,
        &["post", &server.url("/inspect"), "--body", "explicit"],
        b"from stdin",
    );
    assert!(output.status.success());
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].ends_with("explicit"));
    assert!(!requests[0].ends_with("from stdin"));
}

#[test]
fn body_output_mode_writes_only_the_response_body() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(1);

    let output = run_gwab(
        &directory,
        &["get", &server.url("/response"), "--output", "body"],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"response payload");
    assert_eq!(output.stderr, Vec::<u8>::new());
    assert_eq!(server.finish().len(), 1);
}

#[test]
fn all_output_mode_includes_status_headers_and_body() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(1);

    let output = run_gwab(
        &directory,
        &["get", &server.url("/response"), "--output", "all"],
    );
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).expect("fixture response is UTF-8");
    assert!(output.starts_with("HTTP/1.1 200"));
    assert!(output.contains("content-type: text/plain"));
    assert!(output.ends_with("\r\n\r\nresponse payload"));
    assert_eq!(server.finish().len(), 1);
}

fn run_gwab(directory: &TestDirectory, args: &[&str]) -> Output {
    let data_dir = directory.path().join("data");
    let config_dir = directory.path().join("config");
    fs::create_dir_all(&data_dir).expect("test data directory should be created");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");
    Command::new(env!("CARGO_BIN_EXE_gwab"))
        .args(args)
        .current_dir(directory.path())
        .env("XDG_DATA_HOME", data_dir)
        .env("XDG_CONFIG_HOME", config_dir)
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .output()
        .expect("gwab process should start")
}

fn run_gwab_with_stdin(directory: &TestDirectory, args: &[&str], input: &[u8]) -> Output {
    let data_dir = directory.path().join("data");
    let config_dir = directory.path().join("config");
    fs::create_dir_all(&data_dir).expect("test data directory should be created");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");
    let mut process = Command::new(env!("CARGO_BIN_EXE_gwab"))
        .args(args)
        .current_dir(directory.path())
        .env("XDG_DATA_HOME", data_dir)
        .env("XDG_CONFIG_HOME", config_dir)
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("gwab process should start");
    process
        .stdin
        .take()
        .expect("child stdin should be available")
        .write_all(input)
        .expect("input should be sent to gwab");
    process
        .wait_with_output()
        .expect("gwab process output should be collected")
}

#[test]
fn cookies_persist_between_invocations_and_respect_path_and_expiry() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(3);

    let set = run_gwab(&directory, &["get", &server.url("/set")]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let cookie_file = directory.path().join("data/gwab/cookies.json");
    assert!(cookie_file.is_file(), "cookie store should be persisted");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(cookie_file)
            .expect("cookie file metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "cookie file should be private to its owner");
    }

    let private = run_gwab(&directory, &["get", &server.url("/private/check")]);
    assert!(
        private.status.success(),
        "{}",
        String::from_utf8_lossy(&private.stderr)
    );
    let private_body = String::from_utf8_lossy(&private.stdout);
    assert!(private_body.contains("session=abc"));
    assert!(private_body.contains("private=only"));
    assert!(!private_body.contains("expired=never"));

    let root = run_gwab(&directory, &["get", &server.url("/check")]);
    assert!(
        root.status.success(),
        "{}",
        String::from_utf8_lossy(&root.stderr)
    );
    let root_body = String::from_utf8_lossy(&root.stdout);
    assert!(root_body.contains("session=abc"));
    assert!(!root_body.contains("private=only"));

    let requests = server.finish();
    assert_eq!(requests.len(), 3);
}

#[test]
fn post_sends_interpolated_body_and_custom_header() {
    let directory = TestDirectory::new();
    let server = HttpTestServer::start(1);

    let output = run_gwab(
        &directory,
        &[
            "post",
            &server.url("/echo"),
            "--add-header",
            "X-Test: integration",
            "--var",
            "greeting=hello",
            "--body",
            "{{greeting}} world",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = server.finish();
    let request = requests.first().expect("server should receive one request");
    assert!(request.starts_with("POST /echo HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("x-test: integration"));
    assert!(request.ends_with("hello world"));
}

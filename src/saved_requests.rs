//! Named requests stored in the current directory.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const REQUESTS_FILE: &str = "gwab-requests.toml";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct SavedRequest {
    pub(crate) url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(rename = "add-header", skip_serializing_if = "Vec::is_empty")]
    pub(crate) headers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) body: Option<String>,
    #[serde(rename = "var", skip_serializing_if = "Vec::is_empty")]
    pub(crate) vars: Vec<String>,
    #[serde(rename = "env", skip_serializing_if = "Option::is_none")]
    pub(crate) environment: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) verbose: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) quiet: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output: Option<String>,
}

#[derive(Debug)]
pub(crate) struct NamedRequest {
    pub(crate) method: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) request: SavedRequest,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip predicate takes a reference.
fn is_false(value: &bool) -> bool {
    !value
}

pub(crate) fn save(
    cwd: &Path,
    method: &str,
    name: &str,
    request: SavedRequest,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("saved request name cannot be empty".to_owned());
    }

    let path = file_path(cwd);
    let mut requests = load_table(&path)?;
    requests.insert(
        request_key(method, name),
        toml::Value::try_from(request)
            .map_err(|_| "could not serialize saved request".to_owned())?,
    );
    let contents = toml::to_string_pretty(&requests)
        .map_err(|_| "could not serialize saved requests".to_owned())?;
    fs::write(&path, contents)
        .map_err(|_| format!("could not write saved requests file {}", path.display()))
}

pub(crate) fn load(cwd: &Path, method: &str, name: &str) -> Result<SavedRequest, String> {
    let path = file_path(cwd);
    let requests = load_table(&path)?;
    let key = request_key(method, name);
    let value = requests
        .get(&key)
        .ok_or_else(|| format!("saved request not found: {key}"))?;
    value
        .clone()
        .try_into()
        .map_err(|_| format!("saved request {key:?} has invalid properties"))
}

pub(crate) fn list(cwd: &Path) -> Result<Vec<NamedRequest>, String> {
    let path = file_path(cwd);
    let requests = load_table(&path)?;
    let mut named_requests = Vec::with_capacity(requests.len());
    for (key, value) in requests {
        let Some((method, name)) = key.split_once(' ') else {
            return Err(format!("invalid saved request key {key:?}"));
        };
        if !["get", "post", "put", "patch", "delete"].contains(&method) || name.is_empty() {
            return Err(format!("invalid saved request key {key:?}"));
        }
        let request: SavedRequest = value
            .try_into()
            .map_err(|_| format!("saved request {key:?} has invalid properties"))?;
        let description = request
            .description
            .as_deref()
            .filter(|description| !description.trim().is_empty())
            .unwrap_or(&request.url)
            .to_owned();
        named_requests.push(NamedRequest {
            method: method.to_owned(),
            name: name.to_owned(),
            description,
            request,
        });
    }
    named_requests
        .sort_by(|left, right| (&left.method, &left.name).cmp(&(&right.method, &right.name)));
    Ok(named_requests)
}

fn load_table(path: &Path) -> Result<toml::Table, String> {
    if !path.exists() {
        return Ok(toml::Table::new());
    }
    let contents = fs::read_to_string(path)
        .map_err(|_| format!("could not read saved requests file {}", path.display()))?;
    toml::from_str(&contents)
        .map_err(|_| format!("invalid TOML in saved requests file {}", path.display()))
}

fn file_path(cwd: &Path) -> PathBuf {
    cwd.join(REQUESTS_FILE)
}

fn request_key(method: &str, name: &str) -> String {
    format!("{method} {name}")
}

#[cfg(test)]
mod tests {
    use super::{SavedRequest, list, load, save};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-saved-requests")
                .join(format!("{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("test directory should be created");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn saved_request_round_trips_properties_and_names_are_method_scoped() {
        let directory = TestDirectory::new();
        let request = SavedRequest {
            url: "/users".to_owned(),
            description: Some("List users".to_owned()),
            headers: vec!["Accept: application/json".to_owned()],
            body: Some("{\"name\":\"Ada\"}".to_owned()),
            vars: vec!["tenant=7".to_owned()],
            environment: Some("staging".to_owned()),
            verbose: true,
            quiet: true,
            output: Some("body".to_owned()),
        };

        save(&directory.0, "get", "users", request.clone()).expect("request should save");
        save(&directory.0, "post", "users", SavedRequest::default())
            .expect("same name under another method should save");

        let loaded = load(&directory.0, "get", "users").expect("saved GET should load");
        let post = load(&directory.0, "post", "users").expect("saved POST should load");
        assert_eq!(loaded.url, request.url);
        assert_eq!(loaded.description, request.description);
        assert_eq!(loaded.headers, request.headers);
        assert_eq!(loaded.body, request.body);
        assert_eq!(loaded.vars, request.vars);
        assert_eq!(loaded.environment, request.environment);
        assert!(loaded.verbose);
        assert!(loaded.quiet);
        assert_eq!(loaded.output.as_deref(), Some("body"));
        assert_eq!(post.url, "");
    }

    #[test]
    fn saved_request_list_has_a_description_or_uses_the_url() {
        let directory = TestDirectory::new();
        save(
            &directory.0,
            "get",
            "users",
            SavedRequest {
                url: "/users".to_owned(),
                description: Some("List users".to_owned()),
                ..SavedRequest::default()
            },
        )
        .expect("request should save");
        save(
            &directory.0,
            "post",
            "create-user",
            SavedRequest {
                url: "/users".to_owned(),
                ..SavedRequest::default()
            },
        )
        .expect("request without description should save");

        let requests = list(&directory.0).expect("saved requests should list");

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].name, "users");
        assert_eq!(requests[0].description, "List users");
        assert_eq!(requests[1].name, "create-user");
        assert_eq!(requests[1].description, "/users");
    }

    #[test]
    fn malformed_saved_file_is_not_overwritten() {
        let directory = TestDirectory::new();
        let path = directory.0.join("gwab-requests.toml");
        fs::write(&path, "not = [valid").expect("malformed file should be written");

        let error = save(&directory.0, "get", "users", SavedRequest::default())
            .expect_err("malformed request file should be rejected");

        assert!(error.contains("invalid TOML"));
        assert_eq!(
            fs::read_to_string(path).expect("file should remain"),
            "not = [valid"
        );
    }
}

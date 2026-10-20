use crate::ir::{HttpMethod, HttpRequest};
use crate::state::RuntimeState;
use reqwest::blocking::Client;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Sends one request and prints its status and response body.
///
/// # Errors
///
/// Returns a readable error if the URL, headers, request, or response body is
/// invalid or cannot be handled.
pub fn run(http_request: HttpRequest) -> Result<(), String> {
    let HttpRequest {
        method,
        url,
        headers,
        body,
        vars: assignments,
        verbose,
        environment,
    } = http_request;

    let method = match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    };

    let state_variables = RuntimeState::values()?;
    let mut cli_variables = BTreeMap::new();
    for assignment in assignments {
        cli_variables.insert(assignment.name, assignment.value);
    }
    let cwd = std::env::current_dir()
        .map_err(|_| "could not determine the current working directory".to_owned())?;
    let variables = crate::config::resolve_variables(
        &cwd,
        &state_variables,
        &cli_variables,
        environment.as_deref(),
    )?;
    let url = interpolate(&url, &variables)?;
    let url = resolve_url(&url, &variables)?;

    let dns_metrics = Arc::new(Mutex::new(Vec::new()));
    let mut client_builder = Client::builder();
    if verbose {
        client_builder = client_builder.dns_resolver(Arc::new(TimedResolver {
            metrics: Arc::clone(&dns_metrics),
        }));
    }
    let client = client_builder
        .build()
        .map_err(|error| format!("could not create HTTP client: {error}"))?;
    let mut request = client.request(method, &url);

    for header in headers {
        let value = interpolate(&header.value, &variables)?;
        let name = HeaderName::from_str(&header.name)
            .map_err(|error| format!("invalid header name: {error}"))?;
        let value = HeaderValue::from_str(&value)
            .map_err(|_| format!("invalid value for header {:?}", header.name))?;
        request = request.header(name, value);
    }

    let request_body = body
        .map(|body| interpolate(&body, &variables))
        .transpose()?;
    if let Some(body) = &request_body {
        request = request.body(body.clone());
    }

    let request = request
        .build()
        .map_err(|_| "could not build HTTP request".to_owned())?;
    if verbose {
        eprintln!("> {} {}", request.method(), request.url());
        let headers = header_lines(request.headers());
        if !headers.is_empty() {
            eprintln!("{headers}");
        }
        eprintln!(
            "Request body:\n{}",
            request_body.as_deref().unwrap_or("<none>")
        );
    }

    let started = Instant::now();
    let response = client.execute(request);
    let response = match response {
        Ok(response) => response,
        Err(_) => {
            if verbose {
                print_metrics(&dns_metrics, started.elapsed());
            }
            return Err("request failed while contacting the server".to_owned());
        }
    };
    let status = response.status();
    let response_headers = response.headers().clone();
    if verbose {
        eprintln!("< {status}");
        let headers = header_lines(&response_headers);
        if !headers.is_empty() {
            eprintln!("{headers}");
        }
    }
    let body = match response.text() {
        Ok(body) => body,
        Err(_) => {
            if verbose {
                print_metrics(&dns_metrics, started.elapsed());
            }
            return Err("could not read response body".to_owned());
        }
    };
    if verbose {
        eprintln!("Response body:\n{body}");
        print_metrics(&dns_metrics, started.elapsed());
    }
    println!("{status}");
    if !body.is_empty() {
        println!("{body}");
    }
    Ok(())
}

struct TimedResolver {
    metrics: Arc<Mutex<Vec<(String, Duration)>>>,
}

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let metrics = Arc::clone(&self.metrics);
        Box::pin(async move {
            let started = Instant::now();
            let result = tokio::net::lookup_host((host.as_str(), 0)).await;
            if let Ok(mut metrics) = metrics.lock() {
                metrics.push((host.clone(), started.elapsed()));
            }
            result
                .map(|addresses| Box::new(addresses.collect::<Vec<_>>().into_iter()) as Addrs)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

fn header_lines(headers: &HeaderMap) -> String {
    headers
        .iter()
        .map(|(name, value)| {
            format!(
                "{name}: {}",
                value.to_str().unwrap_or("<non-text header value>")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_dns_metrics(metrics: &[(String, Duration)]) -> String {
    if metrics.is_empty() {
        return "DNS resolution: no lookup observed (literal IP or resolver cache)".to_owned();
    }
    metrics
        .iter()
        .map(|(host, duration)| {
            format!(
                "DNS lookup {host}: {:.2} ms",
                duration.as_secs_f64() * 1000.0
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_metrics(metrics: &Arc<Mutex<Vec<(String, Duration)>>>, latency: Duration) {
    let dns_metrics = metrics
        .lock()
        .map(|metrics| metrics.clone())
        .unwrap_or_default();
    eprintln!("{}", format_dns_metrics(&dns_metrics));
    eprintln!("Total latency: {:.2} ms", latency.as_secs_f64() * 1000.0);
}

fn interpolate(input: &str, variables: &BTreeMap<String, String>) -> Result<String, String> {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(start) = remaining.find("{{") {
        output.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];
        let end = after_open
            .find("}}")
            .ok_or_else(|| "unterminated variable interpolation".to_owned())?;
        let name = &after_open[..end];
        if name.is_empty() {
            return Err("variable name in interpolation cannot be empty".into());
        }
        let value = variables
            .get(name)
            .ok_or_else(|| format!("missing variable: {name}"))?;
        output.push_str(value);
        remaining = &after_open[end + 2..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn resolve_url(input: &str, variables: &BTreeMap<String, String>) -> Result<String, String> {
    if reqwest::Url::parse(input).is_ok() {
        return Ok(input.to_owned());
    }
    let base = variables
        .get("base_url")
        .ok_or_else(|| "relative URL requires a saved base_url variable".to_owned())?;
    let base = reqwest::Url::parse(base)
        .map_err(|_| "saved base_url is not a valid absolute URL".to_owned())?;
    base.join(input)
        .map(|url| url.into())
        .map_err(|_| "could not resolve request URL against base_url".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{TimedResolver, format_dns_metrics, header_lines, interpolate, resolve_url};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn interpolation_replaces_saved_variables() {
        let variables = BTreeMap::from([("tenant".to_owned(), "12".to_owned())]);

        assert_eq!(
            interpolate("/tenants/{{tenant}}", &variables),
            Ok("/tenants/12".to_owned())
        );
    }

    #[test]
    fn interpolation_reports_missing_variables() {
        let error = interpolate("/users/{{user_id}}", &BTreeMap::new()).unwrap_err();

        assert_eq!(error, "missing variable: user_id");
    }

    #[test]
    fn relative_urls_resolve_against_base_url() {
        let variables =
            BTreeMap::from([("base_url".to_owned(), "https://api.example.com".to_owned())]);

        assert_eq!(
            resolve_url("/posts", &variables),
            Ok("https://api.example.com/posts".to_owned())
        );
    }

    #[test]
    fn verbose_header_dump_includes_all_request_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-tenant", "north".parse().expect("valid header value"));
        headers.insert(
            "authorization",
            "Bearer debug-token".parse().expect("valid header value"),
        );

        let output = header_lines(&headers);

        assert!(output.contains("x-tenant: north"));
        assert!(output.contains("authorization: Bearer debug-token"));
    }

    #[test]
    fn verbose_dns_metrics_include_resolved_host_and_duration() {
        let metrics = vec![("api.example.com".to_owned(), Duration::from_millis(17))];

        let output = format_dns_metrics(&metrics);

        assert!(output.contains("api.example.com"));
        assert!(output.contains("17.00 ms"));
    }

    #[test]
    fn timed_resolver_records_localhost_dns_lookup() {
        let metrics = Arc::new(Mutex::new(Vec::new()));
        let client = reqwest::blocking::Client::builder()
            .dns_resolver(Arc::new(TimedResolver {
                metrics: Arc::clone(&metrics),
            }))
            .build()
            .expect("client should build with timed DNS resolver");

        let _ = client.get("http://localhost:1").send();

        assert!(
            !metrics
                .lock()
                .expect("metrics mutex should not poison")
                .is_empty()
        );
    }
}

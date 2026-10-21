use crate::cookies::PersistentCookieStore;
use crate::ir::{HttpMethod, PreparedRequest};
use crate::render;
use reqwest::blocking::Client;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{HeaderName, HeaderValue};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Sends one request and prints its status and response body unless quiet mode
/// is enabled.
///
/// # Errors
///
/// Returns a readable error if the URL, headers, request, or response body is
/// invalid or cannot be handled.
pub fn run(http_request: PreparedRequest) -> Result<(), String> {
    let PreparedRequest {
        method,
        url,
        headers,
        body,
        verbose,
        quiet,
        output,
        ..
    } = http_request;

    let method = match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    };

    let dns_metrics = Arc::new(Mutex::new(Vec::new()));
    let cookie_store = Arc::new(PersistentCookieStore::load()?);
    let mut client_builder = Client::builder().cookie_provider(Arc::clone(&cookie_store));
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
        let name = HeaderName::from_str(&header.name)
            .map_err(|error| format!("invalid header name: {error}"))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|_| format!("invalid value for header {:?}", header.name))?;
        request = request.header(name, value);
    }

    let request_body = body;
    if let Some(body) = &request_body {
        request = request.body(body.clone());
    }

    let request = request
        .build()
        .map_err(|_| "could not build HTTP request".to_owned())?;
    if verbose && !quiet {
        render::request(&request, request_body.as_deref());
    }

    let started = Instant::now();
    let response = client.execute(request);
    if let Some(error) = cookie_store.take_persistence_error() {
        return Err(error.to_owned());
    }
    let Ok(response) = response else {
        if verbose && !quiet {
            render::metrics(&metrics_snapshot(&dns_metrics), started.elapsed());
        }
        return Err("request failed while contacting the server".to_owned());
    };
    let status = response.status();
    let version = response.version();
    let headers = response.headers().clone();
    let Ok(body) = response.bytes() else {
        if verbose && !quiet {
            render::metrics(&metrics_snapshot(&dns_metrics), started.elapsed());
        }
        return Err("could not read response body".to_owned());
    };
    let request_metrics = metrics_snapshot(&dns_metrics);
    render::response(
        render::ResponseData {
            status,
            version,
            headers: &headers,
            body: &body,
        },
        render::ResponseOptions {
            quiet,
            verbose,
            output,
        },
        render::RequestMetrics {
            dns: &request_metrics,
            latency: started.elapsed(),
        },
    )
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

fn metrics_snapshot(metrics: &Arc<Mutex<Vec<(String, Duration)>>>) -> Vec<(String, Duration)> {
    metrics
        .lock()
        .map(|metrics| metrics.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::TimedResolver;
    use std::sync::{Arc, Mutex};

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

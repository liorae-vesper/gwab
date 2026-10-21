//! Response presentation, kept separate from request transport.

use crate::ir::ResponseOutput;
use reqwest::header::HeaderMap;
use std::io::Write;
use std::time::Duration;

#[derive(Clone, Copy)]
pub(crate) struct ResponseData<'a> {
    pub(crate) status: reqwest::StatusCode,
    pub(crate) version: reqwest::Version,
    pub(crate) headers: &'a HeaderMap,
    pub(crate) body: &'a [u8],
}

#[derive(Clone, Copy)]
pub(crate) struct ResponseOptions {
    pub(crate) quiet: bool,
    pub(crate) verbose: bool,
    pub(crate) output: ResponseOutput,
}

#[derive(Clone, Copy)]
pub(crate) struct RequestMetrics<'a> {
    pub(crate) dns: &'a [(String, Duration)],
    pub(crate) latency: Duration,
}

pub(crate) fn response(
    response: ResponseData<'_>,
    options: ResponseOptions,
    request_metrics: RequestMetrics<'_>,
) -> Result<(), String> {
    let ResponseData {
        status,
        version,
        headers,
        body,
    } = response;
    let ResponseOptions {
        quiet,
        verbose,
        output,
    } = options;
    if verbose && !quiet {
        eprintln!("< {status}");
        let headers = header_lines(headers);
        if !headers.is_empty() {
            eprintln!("{headers}");
        }
        eprintln!("Response body:\n{}", String::from_utf8_lossy(body));
        metrics(request_metrics.dns, request_metrics.latency);
    }
    if quiet {
        return if status.is_success() {
            Ok(())
        } else {
            Err(format!("request returned HTTP status {status}"))
        };
    }
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    match output {
        ResponseOutput::Default => {
            writeln!(stdout, "{status}")
                .and_then(|()| {
                    if body.is_empty() {
                        Ok(())
                    } else {
                        stdout.write_all(String::from_utf8_lossy(body).as_bytes())?;
                        stdout.write_all(b"\n")
                    }
                })
                .map_err(|_| "could not write response to stdout".to_owned())?;
        }
        ResponseOutput::Body => stdout
            .write_all(body)
            .map_err(|_| "could not write response to stdout".to_owned())?,
        ResponseOutput::All => {
            write!(stdout, "{} {status}\r\n", http_version(version))
                .map_err(|_| "could not write response to stdout".to_owned())?;
            for (name, value) in headers {
                write!(stdout, "{name}: ")
                    .and_then(|()| stdout.write_all(value.as_bytes()))
                    .and_then(|()| stdout.write_all(b"\r\n"))
                    .map_err(|_| "could not write response to stdout".to_owned())?;
            }
            stdout
                .write_all(b"\r\n")
                .and_then(|()| stdout.write_all(body))
                .map_err(|_| "could not write response to stdout".to_owned())?;
        }
    }
    Ok(())
}

pub(crate) fn request(request: &reqwest::blocking::Request, body: Option<&str>) {
    eprintln!("> {} {}", request.method(), request.url());
    let headers = header_lines(request.headers());
    if !headers.is_empty() {
        eprintln!("{headers}");
    }
    eprintln!("Request body:\n{}", body.unwrap_or("<none>"));
}

pub(crate) fn header_lines(headers: &HeaderMap) -> String {
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

pub(crate) fn format_dns_metrics(metrics: &[(String, Duration)]) -> String {
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

pub(crate) fn metrics(metrics: &[(String, Duration)], latency: Duration) {
    eprintln!("{}", format_dns_metrics(metrics));
    eprintln!("Total latency: {:.2} ms", latency.as_secs_f64() * 1000.0);
}

fn http_version(version: reqwest::Version) -> &'static str {
    match version {
        reqwest::Version::HTTP_09 => "HTTP/0.9",
        reqwest::Version::HTTP_10 => "HTTP/1.0",
        reqwest::Version::HTTP_2 => "HTTP/2",
        reqwest::Version::HTTP_3 => "HTTP/3",
        _ => "HTTP/1.1",
    }
}

#[cfg(test)]
mod tests {
    use super::{format_dns_metrics, header_lines};
    use reqwest::header::HeaderMap;
    use std::time::Duration;

    #[test]
    fn verbose_header_dump_includes_all_headers() {
        let mut headers = HeaderMap::new();
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
    fn dns_metrics_include_host_and_duration() {
        let metrics = vec![("api.example.com".to_owned(), Duration::from_millis(17))];

        let output = format_dns_metrics(&metrics);

        assert!(output.contains("api.example.com"));
        assert!(output.contains("17.00 ms"));
    }
}

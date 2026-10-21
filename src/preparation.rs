//! Resolves persistent context and prepares request values before transport.

use crate::ir::{HttpRequest, PreparedRequest};
use crate::state::RuntimeState;
use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Read, Write};

pub(crate) fn prepare(request: HttpRequest) -> Result<PreparedRequest, String> {
    let HttpRequest {
        method,
        url,
        headers,
        body,
        vars: assignments,
        verbose,
        quiet,
        environment,
        output,
    } = request;
    let state_variables = RuntimeState::values()?;
    let mut cli_variables = BTreeMap::new();
    for assignment in assignments {
        cli_variables.insert(assignment.name, assignment.value);
    }
    let cwd = std::env::current_dir()
        .map_err(|_| "could not determine the current working directory".to_owned())?;
    let mut variables = crate::config::resolve_variables(
        &cwd,
        &state_variables,
        &cli_variables,
        environment.as_deref(),
    )?;
    let url = resolve_url(&interpolate(&url, &mut variables, !quiet)?, &variables)?;
    let headers = headers
        .into_iter()
        .map(|mut header| {
            header.value = interpolate(&header.value, &mut variables, !quiet)?;
            Ok(header)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let body = read_body(body)?
        .map(|body| interpolate(&body, &mut variables, !quiet))
        .transpose()?;
    Ok(PreparedRequest {
        method,
        url,
        headers,
        body,
        verbose,
        quiet,
        output,
    })
}

pub(crate) fn read_body(body: Option<String>) -> Result<Option<String>, String> {
    if body.is_some() || std::io::stdin().is_terminal() {
        return Ok(body);
    }
    let mut body_from_stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut body_from_stdin)
        .map_err(|_| "could not read request body from stdin as UTF-8".to_owned())?;
    Ok((!body_from_stdin.is_empty()).then_some(body_from_stdin))
}

fn interpolate(
    input: &str,
    variables: &mut BTreeMap<String, String>,
    allow_prompt: bool,
) -> Result<String, String> {
    interpolate_with_prompt(input, variables, |name| {
        if allow_prompt {
            prompt_missing_variable(name)
        } else {
            Err(missing_variable_error(name))
        }
    })
}

fn interpolate_with_prompt<F>(
    input: &str,
    variables: &mut BTreeMap<String, String>,
    mut prompt: F,
) -> Result<String, String>
where
    F: FnMut(&str) -> Result<String, String>,
{
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
        let value = if let Some(value) = variables.get(name) {
            value.clone()
        } else {
            let value = prompt(name)?;
            variables.insert(name.to_owned(), value.clone());
            value
        };
        output.push_str(&value);
        remaining = &after_open[end + 2..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn prompt_missing_variable(name: &str) -> Result<String, String> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(missing_variable_error(name));
    }

    let stdin = std::io::stdin();
    let stderr = std::io::stderr();
    prompt_value(name, &mut stdin.lock(), &mut stderr.lock())
}

fn missing_variable_error(name: &str) -> String {
    format!(
        "missing variable: {name}; set it with --var or in a config file (interactive prompt unavailable)"
    )
}

fn prompt_value(
    name: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<String, String> {
    write!(output, "Enter a value for {name}: ")
        .and_then(|()| output.flush())
        .map_err(|_| "could not display variable prompt".to_owned())?;
    let mut value = String::new();
    let bytes_read = input
        .read_line(&mut value)
        .map_err(|_| "could not read variable value from stdin".to_owned())?;
    if bytes_read == 0 {
        return Err(format!("no value entered for variable: {name}"));
    }
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    Ok(value)
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
        .map(Into::into)
        .map_err(|_| "could not resolve request URL against base_url".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{interpolate, interpolate_with_prompt, prompt_value, resolve_url};
    use std::collections::BTreeMap;
    use std::io::Cursor;

    #[test]
    fn interpolation_replaces_saved_variables() {
        let mut variables = BTreeMap::from([("tenant".to_owned(), "12".to_owned())]);
        assert_eq!(
            interpolate("/tenants/{{tenant}}", &mut variables, false),
            Ok("/tenants/12".to_owned())
        );
    }

    #[test]
    fn interpolation_reports_missing_variables() {
        let error = interpolate("/users/{{user_id}}", &mut BTreeMap::new(), false).unwrap_err();
        assert_eq!(
            error,
            "missing variable: user_id; set it with --var or in a config file (interactive prompt unavailable)"
        );
    }

    #[test]
    fn interpolation_prompts_once_and_reuses_the_value() {
        let mut variables = BTreeMap::new();
        let mut prompted = Vec::new();
        let output = interpolate_with_prompt(
            "/tenants/{{tenant}}/posts/{{tenant}}",
            &mut variables,
            |name| {
                prompted.push(name.to_owned());
                Ok("north".to_owned())
            },
        )
        .expect("missing value should be supplied by the prompt");
        assert_eq!(output, "/tenants/north/posts/north");
        assert_eq!(prompted, ["tenant"]);
        assert_eq!(variables.get("tenant").map(String::as_str), Some("north"));
    }

    #[test]
    fn prompt_reads_one_line_and_removes_the_line_ending() {
        let mut input = Cursor::new(b"north\r\n".to_vec());
        let mut output = Vec::new();
        let value = prompt_value("tenant", &mut input, &mut output)
            .expect("prompt should read the supplied line");
        assert_eq!(value, "north");
        assert_eq!(output, b"Enter a value for tenant: ");
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
}

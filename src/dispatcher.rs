//! Application command dispatch and request preparation.

use crate::cli::{Command, OutputMode, RequestArgs, SavedMethod};
use crate::ir::{Header, HttpMethod, HttpRequest, ResponseOutput, VariableAssigment};
use crate::{engine, preparation, saved_requests, state};
use dialoguer::FuzzySelect;
use std::io::IsTerminal;

/// A command failure together with its output policy.
pub struct DispatchError {
    /// User-facing error description.
    pub message: String,
    /// Whether the command requested that diagnostics be suppressed.
    pub quiet: bool,
}

impl DispatchError {
    fn new(message: String, quiet: bool) -> Self {
        Self { message, quiet }
    }
}

/// Executes one parsed command.
///
/// # Errors
///
/// Returns the user-facing failure and whether it should be suppressed.
pub fn dispatch(command: Command) -> Result<(), DispatchError> {
    match command {
        Command::Get(args) => dispatch_request(args, SavedMethod::Get),
        Command::Post(args) => dispatch_request(args, SavedMethod::Post),
        Command::Put(args) => dispatch_request(args, SavedMethod::Put),
        Command::Patch(args) => dispatch_request(args, SavedMethod::Patch),
        Command::Delete(args) => dispatch_request(args, SavedMethod::Delete),
        Command::Run {
            method: Some(method),
            name: Some(name),
        } => dispatch_saved_request(method, &name),
        Command::Run {
            method: None,
            name: None,
        } => pick_saved_request(),
        Command::Run { .. } => Err(DispatchError::new(
            "provide both METHOD and NAME, or omit both to pick a saved request".to_owned(),
            false,
        )),
        Command::Set { key, value } => state::RuntimeState::set(&key, &value)
            .map_err(|message| DispatchError::new(message, false)),
        Command::Unset { key } => {
            state::RuntimeState::unset(&key).map_err(|message| DispatchError::new(message, false))
        }
        Command::Vars => {
            state::RuntimeState::vars().map_err(|message| DispatchError::new(message, false))
        }
    }
}

fn dispatch_request(args: RequestArgs, method: SavedMethod) -> Result<(), DispatchError> {
    let quiet = args.quiet;
    execute_request(args, method).map_err(|message| DispatchError::new(message, quiet))
}

fn execute_request(args: RequestArgs, method: SavedMethod) -> Result<(), String> {
    if args.description.is_some() && args.save.is_none() {
        return Err("--description requires --save".to_owned());
    }
    let save_name = args.save.clone();
    let mut saved = saved_request_from_args(args);
    saved.body = preparation::read_body(saved.body)?;
    let request = saved_request_to_http_request(&saved, method.into())?;
    if let Some(name) = save_name {
        let cwd = current_directory()?;
        saved_requests::save(&cwd, method.as_str(), &name, saved)?;
    }
    engine::run(preparation::prepare(request)?)
}

fn dispatch_saved_request(method: SavedMethod, name: &str) -> Result<(), DispatchError> {
    let cwd = current_directory().map_err(|message| DispatchError::new(message, false))?;
    let saved = saved_requests::load(&cwd, method.as_str(), name)
        .map_err(|message| DispatchError::new(message, false))?;
    execute_loaded_request(method, &saved)
}

fn pick_saved_request() -> Result<(), DispatchError> {
    let requests = current_directory()
        .and_then(|cwd| saved_requests::list(&cwd))
        .map_err(|message| DispatchError::new(message, false))?;
    if requests.is_empty() {
        return Err(DispatchError::new(
            "no saved requests found in gwab-requests.toml".to_owned(),
            false,
        ));
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(DispatchError::new(
            "saved request picker requires an interactive terminal; specify METHOD and NAME"
                .to_owned(),
            false,
        ));
    }

    let labels = requests.iter().map(saved_request_label).collect::<Vec<_>>();
    let selection = FuzzySelect::new()
        .with_prompt("Select a saved request")
        .items(&labels)
        .interact_opt()
        .map_err(|_| DispatchError::new("could not open saved request picker".to_owned(), false))?;
    let Some(selection) = selection else {
        return Ok(());
    };
    let request = requests.get(selection).ok_or_else(|| {
        DispatchError::new("saved request selection was invalid".to_owned(), false)
    })?;
    let method = SavedMethod::from_name(&request.method).ok_or_else(|| {
        DispatchError::new(
            format!("saved request has unsupported method: {}", request.method),
            false,
        )
    })?;
    execute_loaded_request(method, &request.request)
}

fn execute_loaded_request(
    method: SavedMethod,
    saved: &saved_requests::SavedRequest,
) -> Result<(), DispatchError> {
    let quiet = saved.quiet;
    let result = saved_request_to_http_request(saved, method.into())
        .and_then(preparation::prepare)
        .and_then(engine::run);
    result.map_err(|message| DispatchError::new(message, quiet))
}

fn saved_request_from_args(args: RequestArgs) -> saved_requests::SavedRequest {
    saved_requests::SavedRequest {
        url: args.url,
        description: args.description,
        headers: args.headers,
        body: args.body,
        vars: args.vars,
        environment: args.environment,
        verbose: args.verbose,
        quiet: args.quiet,
        output: args.output.map(|output| match output {
            OutputMode::Body => "body".to_owned(),
            OutputMode::All => "all".to_owned(),
        }),
    }
}

fn saved_request_to_http_request(
    saved: &saved_requests::SavedRequest,
    method: HttpMethod,
) -> Result<HttpRequest, String> {
    if saved.url.is_empty() {
        return Err("saved request has no URL".to_owned());
    }
    let headers = saved
        .headers
        .iter()
        .map(|header| Header::parse(header))
        .collect::<Result<Vec<_>, _>>()?;
    let vars = saved
        .vars
        .iter()
        .map(|var| VariableAssigment::parse(var))
        .collect::<Result<Vec<_>, _>>()?;
    let output = match saved.output.as_deref() {
        Some("body") => ResponseOutput::Body,
        Some("all") => ResponseOutput::All,
        Some(value) => return Err(format!("saved request has invalid output mode: {value}")),
        None => ResponseOutput::Default,
    };
    Ok(HttpRequest {
        method,
        url: saved.url.clone(),
        headers,
        body: saved.body.clone(),
        vars,
        verbose: saved.verbose,
        quiet: saved.quiet,
        environment: saved.environment.clone(),
        output,
    })
}

fn current_directory() -> Result<std::path::PathBuf, String> {
    std::env::current_dir()
        .map_err(|_| "could not determine the current working directory".to_owned())
}

fn saved_request_label(request: &saved_requests::NamedRequest) -> String {
    format!(
        "{:<32} {}",
        format!("{} {}", request.method, request.name),
        request.description
    )
}

#[cfg(test)]
mod tests {
    use super::{execute_loaded_request, saved_request_label};
    use crate::cli::SavedMethod;
    use crate::saved_requests::{NamedRequest, SavedRequest};

    #[test]
    fn quiet_setting_is_kept_when_a_saved_request_fails() {
        let error = execute_loaded_request(
            SavedMethod::Get,
            &SavedRequest {
                url: "https://example.com".to_owned(),
                quiet: true,
                output: Some("invalid".to_owned()),
                ..SavedRequest::default()
            },
        )
        .expect_err("the invalid saved response mode should return a command error");

        assert!(error.quiet);
    }

    #[test]
    fn picker_label_places_the_description_after_the_request_name() {
        let request = NamedRequest {
            method: "get".to_owned(),
            name: "users".to_owned(),
            description: "List active users".to_owned(),
            request: SavedRequest::default(),
        };

        let label = saved_request_label(&request);

        assert!(label.starts_with("get users"));
        assert!(label.ends_with("List active users"));
    }
}

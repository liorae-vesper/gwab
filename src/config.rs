use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

type Variables = BTreeMap<String, String>;
type Environments = BTreeMap<String, Variables>;

#[derive(Default)]
struct ConfigFile {
    variables: Variables,
    environments: Environments,
}

/// Loads global and nearest-project values, then applies saved state, a
/// selected environment, and command-line overrides by key.
pub(crate) fn resolve_variables(
    cwd: &Path,
    state: &Variables,
    cli: &Variables,
    environment: Option<&str>,
) -> Result<Variables, String> {
    let global = global_config_path()
        .filter(|path| path.is_file())
        .map(|path| load_config_file(&path))
        .transpose()?
        .unwrap_or_default();
    let project = nearest_project_config(cwd, Path::is_file)
        .map(|path| load_config_file(&path))
        .transpose()?
        .unwrap_or_default();
    let ordinary = merge_layers(
        &global.variables,
        &project.variables,
        state,
        &Variables::new(),
    );
    let environments = merge_environments(&global.environments, &project.environments);
    let mut resolved = merge_selected_environment(&ordinary, &environments, environment)?;
    resolved.extend(cli.clone());
    Ok(resolved)
}

fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("gwab").join("config.toml"))
}

fn load_config_file(path: &Path) -> Result<ConfigFile, String> {
    let contents = fs::read_to_string(path)
        .map_err(|_| format!("could not read gwab config file {}", path.display()))?;
    parse_config(&contents)
        .map_err(|error| format!("invalid gwab config file {}: {error}", path.display()))
}

fn parse_config(contents: &str) -> Result<ConfigFile, String> {
    let table: toml::Table =
        toml::from_str(contents).map_err(|_| "configuration must contain valid TOML".to_owned())?;
    let mut config = ConfigFile::default();
    let mut nested_vars = Variables::new();
    for (key, value) in table {
        match key.as_str() {
            "vars" => {
                let values = value
                    .as_table()
                    .ok_or_else(|| "the [vars] setting must be a TOML table".to_owned())?;
                parse_variables(values, &mut nested_vars)?;
            }
            "env" => {
                let environments = value
                    .as_table()
                    .ok_or_else(|| "the [env] setting must be a TOML table".to_owned())?;
                for (name, values) in environments {
                    let values = values
                        .as_table()
                        .ok_or_else(|| format!("environment {name:?} must contain a TOML table"))?;
                    config
                        .environments
                        .insert(name.clone(), parse_environment(values, name)?);
                }
            }
            _ => {
                config
                    .variables
                    .insert(key.clone(), scalar_value(&key, &value)?);
            }
        }
    }
    config.variables.extend(nested_vars);
    Ok(config)
}

fn parse_variables(
    values: &toml::map::Map<String, toml::Value>,
    output: &mut Variables,
) -> Result<(), String> {
    for (name, value) in values {
        output.insert(name.clone(), scalar_value(name, value)?);
    }
    Ok(())
}

fn parse_environment(
    values: &toml::map::Map<String, toml::Value>,
    environment: &str,
) -> Result<Variables, String> {
    let mut variables = Variables::new();
    let mut nested_vars = Variables::new();
    for (name, value) in values {
        if name == "vars" {
            let values = value.as_table().ok_or_else(|| {
                format!("the vars setting for environment {environment:?} must be a TOML table")
            })?;
            parse_variables(values, &mut nested_vars)?;
        } else {
            variables.insert(name.clone(), scalar_value(name, value)?);
        }
    }
    variables.extend(nested_vars);
    Ok(variables)
}

fn scalar_value(key: &str, value: &toml::Value) -> Result<String, String> {
    match value {
        toml::Value::String(value) => Ok(value.clone()),
        toml::Value::Integer(value) => Ok(value.to_string()),
        toml::Value::Float(value) => Ok(value.to_string()),
        toml::Value::Boolean(value) => Ok(value.to_string()),
        toml::Value::Datetime(value) => Ok(value.to_string()),
        toml::Value::Array(_) | toml::Value::Table(_) => Err(format!(
            "configuration variable {key:?} must have a scalar value"
        )),
    }
}

fn nearest_project_config<F>(cwd: &Path, mut is_file: F) -> Option<PathBuf>
where
    F: FnMut(&Path) -> bool,
{
    cwd.ancestors()
        .map(|directory| directory.join("gwab.toml"))
        .find(|path| is_file(path))
}

fn merge_layers(
    global: &Variables,
    project: &Variables,
    state: &Variables,
    cli: &Variables,
) -> Variables {
    let mut merged = global.clone();
    merged.extend(project.clone());
    merged.extend(state.clone());
    merged.extend(cli.clone());
    merged
}

fn merge_environments(global: &Environments, project: &Environments) -> Environments {
    let mut environments = global.clone();
    for (name, project_variables) in project {
        environments
            .entry(name.clone())
            .or_default()
            .extend(project_variables.clone());
    }
    environments
}

fn merge_selected_environment(
    ordinary: &Variables,
    environments: &Environments,
    selected: Option<&str>,
) -> Result<Variables, String> {
    let mut resolved = ordinary.clone();
    if let Some(name) = selected {
        let variables = environments
            .get(name)
            .ok_or_else(|| format!("unknown environment: {name}"))?;
        resolved.extend(variables.clone());
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::{
        merge_environments, merge_layers, merge_selected_environment, nearest_project_config,
        parse_config,
    };
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn parses_flat_and_vars_table_values() {
        let config = parse_config(
            r#"
base_url = "https://global.example"

[vars]
tenant = 7
enabled = true
"#,
        )
        .expect("valid TOML config should parse");

        assert_eq!(
            config.variables.get("base_url"),
            Some(&"https://global.example".to_owned())
        );
        assert_eq!(config.variables.get("tenant"), Some(&"7".to_owned()));
        assert_eq!(config.variables.get("enabled"), Some(&"true".to_owned()));
    }

    #[test]
    fn config_layers_merge_by_key_in_precedence_order() {
        let global = BTreeMap::from([
            ("base_url".to_owned(), "https://global.example".to_owned()),
            ("tenant".to_owned(), "1".to_owned()),
        ]);
        let project = BTreeMap::from([
            ("base_url".to_owned(), "https://project.example".to_owned()),
            ("version".to_owned(), "v2".to_owned()),
        ]);
        let state = BTreeMap::from([("tenant".to_owned(), "2".to_owned())]);
        let cli = BTreeMap::from([("tenant".to_owned(), "3".to_owned())]);

        let resolved = merge_layers(&global, &project, &state, &cli);

        assert_eq!(
            resolved.get("base_url"),
            Some(&"https://project.example".to_owned())
        );
        assert_eq!(resolved.get("tenant"), Some(&"3".to_owned()));
        assert_eq!(resolved.get("version"), Some(&"v2".to_owned()));
    }

    #[test]
    fn nearest_project_config_wins_over_parent_config() {
        let cwd = Path::new("/repo/service/nested");
        let project_file = nearest_project_config(cwd, |path| {
            path == Path::new("/repo/gwab.toml") || path == Path::new("/repo/service/gwab.toml")
        });

        assert_eq!(project_file, Some(PathBuf::from("/repo/service/gwab.toml")));
    }

    #[test]
    fn selected_environment_overrides_regular_values_by_key() {
        let ordinary = BTreeMap::from([
            ("base_url".to_owned(), "https://default.example".to_owned()),
            ("tenant".to_owned(), "1".to_owned()),
            ("keep".to_owned(), "yes".to_owned()),
        ]);
        let environments = BTreeMap::from([(
            "staging".to_owned(),
            BTreeMap::from([
                ("base_url".to_owned(), "https://staging.example".to_owned()),
                ("tenant".to_owned(), "2".to_owned()),
            ]),
        )]);

        let resolved = merge_selected_environment(&ordinary, &environments, Some("staging"))
            .expect("known environment should resolve");

        assert_eq!(
            resolved.get("base_url").map(String::as_str),
            Some("https://staging.example")
        );
        assert_eq!(resolved.get("tenant").map(String::as_str), Some("2"));
        assert_eq!(resolved.get("keep").map(String::as_str), Some("yes"));
    }

    #[test]
    fn selected_environment_must_exist() {
        let error = merge_selected_environment(&BTreeMap::new(), &BTreeMap::new(), Some("missing"))
            .expect_err("unknown environment should fail");

        assert_eq!(error, "unknown environment: missing");
    }

    #[test]
    fn project_environment_values_override_global_environment_values_by_key() {
        let global = BTreeMap::from([(
            "staging".to_owned(),
            BTreeMap::from([
                ("base_url".to_owned(), "https://global.example".to_owned()),
                ("tenant".to_owned(), "1".to_owned()),
            ]),
        )]);
        let project = BTreeMap::from([(
            "staging".to_owned(),
            BTreeMap::from([("tenant".to_owned(), "2".to_owned())]),
        )]);

        let resolved = merge_environments(&global, &project);

        assert_eq!(
            resolved["staging"].get("base_url").map(String::as_str),
            Some("https://global.example")
        );
        assert_eq!(
            resolved["staging"].get("tenant").map(String::as_str),
            Some("2")
        );
    }

    #[test]
    fn parses_named_environment_variables() {
        let config = parse_config(
            r#"
[env.staging]
base_url = "https://staging.example"

[env.production.vars]
base_url = "https://api.example"
tenant = 9
"#,
        )
        .expect("valid environment config should parse");

        assert_eq!(
            config.environments["staging"]
                .get("base_url")
                .map(String::as_str),
            Some("https://staging.example")
        );
        assert_eq!(
            config.environments["production"]
                .get("tenant")
                .map(String::as_str),
            Some("9")
        );
    }
}

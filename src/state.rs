use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::PathBuf;

pub(crate) struct RuntimeState {
    settings_path: PathBuf,
    scope: String,
}

type ScopedData = BTreeMap<String, BTreeMap<String, String>>;
type JsonData = BTreeMap<String, String>;
impl RuntimeState {
    pub fn new() -> Result<Self, io::Error> {
        let path = Self::get_or_init()?;
        let scope = std::env::current_dir()?.to_string_lossy().into_owned();
        Ok(RuntimeState {
            settings_path: path,
            scope,
        })
    }

    fn get_or_init() -> Result<PathBuf, io::Error> {
        let path = dirs::data_dir().ok_or(io::Error::new(
            ErrorKind::NotFound,
            "the data directory could not be found in this OS",
        ))?;
        let ided_path = path.join("gwab");
        fs::create_dir_all(&ided_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&ided_path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(ided_path)
    }

    fn settings_file(&self) -> PathBuf {
        self.settings_path.join("settings.json")
    }

    fn read_file(&self) -> Result<ScopedData, io::Error> {
        let path = self.settings_file();
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(error) => return Err(error),
        };
        parse_settings(&contents, &self.scope)
    }

    fn write_file(&self, data: &ScopedData) -> Result<(), io::Error> {
        let contents = serde_json::to_string_pretty(data)
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?;
        let path = self.settings_file();
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path)?.write_all(contents.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn set(name: String, value: String) -> Result<(), String> {
        let state = Self::new().map_err(|error| format!("could not open gwab state: {error}"))?;
        let mut data = state
            .read_file()
            .map_err(|error| format!("could not read gwab state: {error}"))?;
        set_scoped_value(&mut data, &state.scope, &name, &value);
        state
            .write_file(&data)
            .map_err(|error| format!("could not save gwab state: {error}"))
    }

    pub(crate) fn unset(key: String) -> Result<(), String> {
        let state = Self::new().map_err(|error| format!("could not open gwab state: {error}"))?;
        let mut data = state
            .read_file()
            .map_err(|error| format!("could not read gwab state: {error}"))?;
        if let Some(values) = data.get_mut(&state.scope) {
            values.remove(&key);
        }
        state
            .write_file(&data)
            .map_err(|error| format!("could not save gwab state: {error}"))
    }

    pub(crate) fn vars() -> Result<(), String> {
        let state = Self::new().map_err(|error| format!("could not open gwab state: {error}"))?;
        let data = state
            .read_file()
            .map_err(|error| format!("could not read gwab state: {error}"))?;
        for (key, value) in values_for_scope(&data, &state.scope) {
            if is_sensitive_name(&key) {
                println!("{key}=<redacted>");
            } else {
                println!("{key}={value}");
            }
        }
        Ok(())
    }

    pub(crate) fn values() -> Result<JsonData, String> {
        let state = Self::new().map_err(|error| format!("could not open gwab state: {error}"))?;
        state
            .read_file()
            .map(|data| values_for_scope(&data, &state.scope))
            .map_err(|error| format!("could not read gwab state: {error}"))
    }
}

fn parse_settings(contents: &str, scope: &str) -> Result<ScopedData, io::Error> {
    let value: serde_json::Value = serde_json::from_str(contents)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    if value
        .as_object()
        .is_some_and(|object| object.values().all(serde_json::Value::is_string))
    {
        let legacy: JsonData = serde_json::from_value(value)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        return Ok(BTreeMap::from([(scope.to_owned(), legacy)]));
    }
    serde_json::from_value(value).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn set_scoped_value(data: &mut ScopedData, scope: &str, key: &str, value: &str) {
    data.entry(scope.to_owned())
        .or_default()
        .insert(key.to_owned(), value.to_owned());
}

fn values_for_scope(data: &ScopedData, scope: &str) -> JsonData {
    data.get(scope).cloned().unwrap_or_default()
}

fn is_sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "credential",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|marker| name.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::{ScopedData, parse_settings, set_scoped_value, values_for_scope};

    #[test]
    fn variables_are_isolated_by_directory_scope() {
        let mut data = ScopedData::new();
        set_scoped_value(&mut data, "/work/alpha", "tenant", "11");
        set_scoped_value(&mut data, "/work/beta", "tenant", "22");

        assert_eq!(
            values_for_scope(&data, "/work/alpha").get("tenant"),
            Some(&"11".to_owned())
        );
        assert_eq!(
            values_for_scope(&data, "/work/beta").get("tenant"),
            Some(&"22".to_owned())
        );
        assert!(values_for_scope(&data, "/work/gamma").is_empty());
    }

    #[test]
    fn legacy_flat_settings_are_migrated_to_the_current_scope() {
        let data = parse_settings(r#"{"tenant":"7"}"#, "/work/alpha")
            .expect("valid legacy JSON should parse");

        assert_eq!(
            values_for_scope(&data, "/work/alpha").get("tenant"),
            Some(&"7".to_owned())
        );
        assert!(values_for_scope(&data, "/work/beta").is_empty());
    }

    #[test]
    fn scoped_settings_round_trip_through_nested_json() {
        let mut data = ScopedData::new();
        set_scoped_value(
            &mut data,
            "/work/alpha",
            "base_url",
            "https://alpha.example",
        );
        set_scoped_value(&mut data, "/work/beta", "base_url", "https://beta.example");
        let serialized = serde_json::to_string(&data).expect("scoped settings should serialize");
        let restored =
            parse_settings(&serialized, "/work/other").expect("scoped settings should parse");

        assert_eq!(
            values_for_scope(&restored, "/work/alpha").get("base_url"),
            Some(&"https://alpha.example".to_owned())
        );
        assert_eq!(
            values_for_scope(&restored, "/work/beta").get("base_url"),
            Some(&"https://beta.example".to_owned())
        );
    }
}

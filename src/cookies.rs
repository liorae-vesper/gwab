use cookie_store::{CookieStore as CookieJar, RawCookie};
use reqwest::Url;
use reqwest::cookie::CookieStore;
use reqwest::header::HeaderValue;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, ErrorKind, Write};
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) struct PersistentCookieStore {
    path: PathBuf,
    cookies: Mutex<CookieJar>,
    persistence_error: Mutex<Option<&'static str>>,
}

impl PersistentCookieStore {
    pub(crate) fn load() -> Result<Self, String> {
        let directory = crate::state::RuntimeState::app_data_dir()
            .map_err(|_| "could not access the gwab data directory")?;
        let path = directory.join("cookies.json");
        let cookies = match File::open(&path) {
            Ok(file) => cookie_store::serde::json::load_all(BufReader::new(file))
                .map_err(|_| "could not read the gwab cookie store")?,
            Err(error) if error.kind() == ErrorKind::NotFound => CookieJar::default(),
            Err(_) => return Err("could not read the gwab cookie store".to_owned()),
        };
        Ok(Self {
            path,
            cookies: Mutex::new(cookies),
            persistence_error: Mutex::new(None),
        })
    }

    fn persist(&self, cookies: &CookieJar) -> Result<(), &'static str> {
        let mut contents = Vec::new();
        cookie_store::serde::json::save_incl_expired_and_nonpersistent(cookies, &mut contents)
            .map_err(|_| "could not serialize the gwab cookie store")?;
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&self.path)
            .map_err(|_| "could not save the gwab cookie store")?;
        file.write_all(&contents)
            .map_err(|_| "could not save the gwab cookie store")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
                .map_err(|_| "could not secure the gwab cookie store")?;
        }
        Ok(())
    }

    fn remember_persistence_error(&self, error: Option<&'static str>) {
        if let Ok(mut last_error) = self.persistence_error.lock() {
            *last_error = error;
        }
    }

    pub(crate) fn take_persistence_error(&self) -> Option<&'static str> {
        self.persistence_error
            .lock()
            .ok()
            .and_then(|mut error| error.take())
    }
}

impl CookieStore for PersistentCookieStore {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let parsed = headers.filter_map(|header| {
            let raw = header.to_str().ok()?;
            RawCookie::parse(raw).ok().map(RawCookie::into_owned)
        });
        let Ok(mut cookies) = self.cookies.lock() else {
            self.remember_persistence_error(Some("could not update the gwab cookie store"));
            return;
        };
        cookies.store_response_cookies(parsed, url);
        let error = self.persist(&cookies).err();
        self.remember_persistence_error(error);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let cookies = self.cookies.lock().ok()?;
        let value = cookies
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        if value.is_empty() {
            None
        } else {
            HeaderValue::from_str(&value).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CookieJar, PersistentCookieStore};
    use reqwest::cookie::CookieStore;
    use reqwest::header::HeaderValue;
    use std::sync::Mutex;

    #[test]
    fn cookie_store_selects_cookies_by_path_and_secure_flag() {
        let url = reqwest::Url::parse("http://api.example.test/private/start")
            .expect("test URL should parse");
        let headers = [
            HeaderValue::from_static("session=abc; Path=/; HttpOnly"),
            HeaderValue::from_static("private=only; Path=/private"),
            HeaderValue::from_static("secure=no-http; Path=/; Secure"),
            HeaderValue::from_static("domain=match; Domain=example.test; Path=/"),
            HeaderValue::from_static("wrong=other; Domain=other.test; Path=/"),
        ];
        let mut headers = headers.iter();
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("cookie-test-{}.json", std::process::id()));
        let store = PersistentCookieStore {
            path: path.clone(),
            cookies: Mutex::new(CookieJar::default()),
            persistence_error: Mutex::new(None),
        };

        store.set_cookies(&mut headers, &url);
        assert_eq!(store.take_persistence_error(), None);

        let private = store
            .cookies(&url)
            .and_then(|header| header.to_str().ok().map(str::to_owned))
            .expect("matching cookies should be available");
        assert!(private.contains("session=abc"));
        assert!(private.contains("private=only"));
        assert!(private.contains("domain=match"));
        assert!(!private.contains("secure=no-http"));
        assert!(!private.contains("wrong=other"));

        let root = reqwest::Url::parse("http://api.example.test/").expect("test URL should parse");
        let root_cookies = store
            .cookies(&root)
            .and_then(|header| header.to_str().ok().map(str::to_owned))
            .expect("root cookie should be available");
        assert!(root_cookies.contains("session=abc"));
        assert!(!root_cookies.contains("private=only"));
        let _ = std::fs::remove_file(path);
    }
}

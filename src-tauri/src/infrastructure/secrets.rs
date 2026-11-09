use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::{AppError, SecretPort};

pub const FACTORIAL_KEY: &str = "factorial-api-key";
pub const CLICKUP_KEY: &str = "clickup-api-key";

/// First characters of a stored key, then a fixed mask. The rest of the secret stays in the keychain.
pub fn mask_secret(value: &str) -> String {
    let chars: Vec<char> = value.trim().chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let visible = match chars.len() {
        0..=4 => 0,
        5..=8 => 2,
        _ => 5,
    };
    let prefix: String = chars.into_iter().take(visible).collect();
    format!("{prefix}**********")
}

pub struct KeyringSecrets {
    path: Option<PathBuf>,
}

impl KeyringSecrets {
    pub fn new(config_dir: Option<&Path>) -> Self {
        Self {
            path: config_dir.map(|dir| dir.join("session.json")),
        }
    }
}

impl SecretPort for KeyringSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        if let Some(value) = self.stored(key) {
            return Ok(Some(value));
        }
        match keychain_get(key)? {
            Some(value) => {
                let _ = self.remember(key, &value);
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.remember(key, value)?;
        if let Err(err) = keychain_set(key, value) {
            if self.path.is_none() {
                return Err(err);
            }
        }
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            let mut values = read_session(path);
            values.remove(key);
            write_session(path, &values)?;
        }
        let _ = keychain_delete(key);
        Ok(())
    }
}

impl KeyringSecrets {
    fn stored(&self, key: &str) -> Option<String> {
        let path = self.path.as_ref()?;
        read_session(path)
            .get(key)
            .filter(|value| !value.is_empty())
            .cloned()
    }

    fn remember(&self, key: &str, value: &str) -> Result<(), AppError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut values = read_session(path);
        values.insert(key.to_string(), value.to_string());
        write_session(path, &values)
    }
}

fn read_session(path: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_session(path: &Path, values: &BTreeMap<String, String>) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| AppError::message(format!("No se pudo crear la carpeta de la sesión: {err}")))?;
    }
    let text = serde_json::to_string(values).unwrap_or_else(|_| "{}".into());
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| AppError::message(format!("No se pudo guardar la sesión: {err}")))?;
    file.write_all(text.as_bytes())
        .map_err(|err| AppError::message(format!("No se pudo guardar la sesión: {err}")))?;
    Ok(())
}

#[cfg(not(test))]
fn entry(key: &str) -> Result<keyring::Entry, AppError> {
    keyring::Entry::new("app.ritmo.desktop", key)
        .map_err(|err| AppError::message(format!("Llavero no disponible: {err}")))
}

fn keychain_get(key: &str) -> Result<Option<String>, AppError> {
    #[cfg(test)]
    {
        let _ = key;
        return Ok(None);
    }
    #[cfg(not(test))]
    match entry(key)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(AppError::message(format!("No se pudo leer el llavero: {err}"))),
    }
}

fn keychain_set(key: &str, value: &str) -> Result<(), AppError> {
    #[cfg(test)]
    {
        let _ = (key, value);
        return Ok(());
    }
    #[cfg(not(test))]
    entry(key)?
        .set_password(value)
        .map_err(|err| AppError::message(format!("No se pudo guardar en el llavero: {err}")))
}

fn keychain_delete(key: &str) -> Result<(), AppError> {
    #[cfg(test)]
    {
        let _ = key;
        return Ok(());
    }
    #[cfg(not(test))]
    match entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(AppError::message(format!("No se pudo borrar del llavero: {err}"))),
    }
}

#[cfg(test)]
pub struct MemorySecrets {
    values: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl MemorySecrets {
    pub fn new() -> Self {
        Self {
            values: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(test)]
impl SecretPort for MemorySecrets {
    fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        Ok(self.values.lock().expect("secrets").get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.values
            .lock()
            .expect("secrets")
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), AppError> {
        self.values.lock().expect("secrets").remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_shows_a_short_prefix() {
        assert_eq!(mask_secret("  "), "");
        assert_eq!(mask_secret("abcd"), "**********");
        assert_eq!(mask_secret("pk_12345"), "pk**********");
        assert_eq!(mask_secret("mk_yw-live-token"), "mk_yw**********");
    }

    #[test]
    fn memory_secrets_roundtrip() {
        let secrets = MemorySecrets::new();
        assert!(secrets.get("gemini-api-key").unwrap().is_none());
        secrets.set("gemini-api-key", "abc").unwrap();
        assert_eq!(secrets.get("gemini-api-key").unwrap().as_deref(), Some("abc"));
        secrets.delete("gemini-api-key").unwrap();
        assert!(secrets.get("gemini-api-key").unwrap().is_none());
    }

    #[test]
    fn session_file_keeps_the_google_login_across_restarts() {
        let dir = std::env::temp_dir().join(format!("ritmo-session-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let secrets = KeyringSecrets::new(Some(&dir));
        secrets.set("google-refresh", "token-1").unwrap();
        let again = KeyringSecrets::new(Some(&dir));
        assert_eq!(again.get("google-refresh").unwrap().as_deref(), Some("token-1"));
        again.delete("google-refresh").unwrap();
        assert!(again.get("google-refresh").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

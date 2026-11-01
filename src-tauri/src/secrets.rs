//! Secrets storage: the Redmine API key and the Google Chat sign-in.
//!
//! They live in the OS keychain (macOS Keychain, Secret Service on Linux) as
//! one JSON item. Where no keychain is available (a Linux box without a
//! Secret Service, a locked or denied keychain) they fall back to
//! `credentials.json` in the app config dir, readable only by the current
//! user (0600), like the `gh` or `aws` CLIs. A file left by an older version
//! is moved into the keychain on first start.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const SERVICE: &str = "app.redmine-logtime.desktop";
const ACCOUNT: &str = "credentials";

#[derive(Default, Serialize, Deserialize)]
struct Secrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    google: Option<GoogleAuth>,
}

/// A Google sign-in with read-only Chat scopes. The client id/secret belong
/// to the user's company (an "Internal" OAuth client of type Desktop app).
#[derive(Clone, Serialize, Deserialize)]
pub struct GoogleAuth {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    /// OIDC `sub`: the id Google Chat uses for this user's messages.
    pub user_id: String,
    pub email: String,
}

/// Where the secrets are kept, for the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Storage {
    Keychain,
    File,
}

/// A place that holds the secrets as one JSON document.
trait Vault: Send + Sync {
    fn read(&self) -> Result<Option<String>, String>;
    fn write(&self, json: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

struct KeychainVault(keyring::Entry);

impl Vault for KeychainVault {
    fn read(&self) -> Result<Option<String>, String> {
        match self.0.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn write(&self, json: &str) -> Result<(), String> {
        self.0.set_password(json).map_err(|e| e.to_string())
    }

    fn delete(&self) -> Result<(), String> {
        match self.0.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

struct FileVault {
    path: PathBuf,
}

impl Vault for FileVault {
    fn read(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => {
                restrict(&self.path).map_err(|e| format!("không đặt được quyền cho file: {e}"))?;
                Ok(Some(raw))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Writes a private temp file, then renames it over the real one so the
    /// key is never readable by others, even briefly.
    fn write(&self, json: &str) -> Result<(), String> {
        let write = || -> io::Result<()> {
            let tmp = self.path.with_extension("json.tmp");
            let _ = fs::remove_file(&tmp);
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            let mut f = opts.open(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
            fs::rename(&tmp, &self.path)
        };
        write().map_err(|e| e.to_string())
    }

    fn delete(&self) -> Result<(), String> {
        match fs::remove_file(&self.path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}

pub struct Credentials {
    vault: Box<dyn Vault>,
    storage: Storage,
}

impl Credentials {
    /// The OS keychain when it works, else the private file.
    pub fn new(config_dir: &Path) -> Self {
        let keychain = keyring::Entry::new(SERVICE, ACCOUNT)
            .ok()
            .map(|e| Box::new(KeychainVault(e)) as Box<dyn Vault>);
        Self::choose(keychain, file_vault(config_dir))
    }

    /// Only the private file (tests, or when the keychain is not wanted).
    #[cfg(test)]
    pub fn file_only(config_dir: &Path) -> Self {
        Self {
            vault: Box::new(file_vault(config_dir)),
            storage: Storage::File,
        }
    }

    /// Uses the keychain if it answers, moving an older file into it; the
    /// file is only removed once the keychain holds the same content.
    fn choose(keychain: Option<Box<dyn Vault>>, file: FileVault) -> Self {
        let fallback = |why: String| {
            log::warn!("Không dùng được keychain, lưu thông tin đăng nhập trong file: {why}");
            Self {
                vault: Box::new(FileVault {
                    path: file.path.clone(),
                }),
                storage: Storage::File,
            }
        };
        let Some(keychain) = keychain else {
            return fallback("không tạo được mục keychain".into());
        };
        let in_keychain = match keychain.read() {
            Ok(v) => v,
            Err(e) => return fallback(e),
        };
        if in_keychain.is_none()
            && let Ok(Some(old)) = file.read()
        {
            let moved = keychain
                .write(&old)
                .and_then(|_| keychain.read())
                .map(|back| back.as_deref() == Some(old.as_str()));
            match moved {
                Ok(true) => {
                    if let Err(e) = file.delete() {
                        log::warn!("Đã chuyển vào keychain nhưng chưa xoá được file cũ: {e}");
                    } else {
                        log::info!("Đã chuyển thông tin đăng nhập từ file vào keychain");
                    }
                }
                Ok(false) => return fallback("keychain không giữ đúng nội dung".into()),
                Err(e) => return fallback(e),
            }
        }
        Self {
            vault: keychain,
            storage: Storage::Keychain,
        }
    }

    pub fn storage(&self) -> Storage {
        self.storage
    }

    pub fn get_api_key(&self) -> Result<Option<String>, String> {
        let s = self.load().map_err(|e| format!("Không đọc được API key: {e}"))?;
        Ok(s.api_key.filter(|k| !k.trim().is_empty()))
    }

    pub fn set_api_key(&self, key: &str) -> Result<(), String> {
        self.modify(|s| s.api_key = Some(key.trim().to_string()))
            .map_err(|e| format!("Không lưu được API key: {e}"))
    }

    pub fn delete_api_key(&self) -> Result<(), String> {
        self.modify(|s| s.api_key = None)
            .map_err(|e| format!("Không xoá được API key: {e}"))
    }

    pub fn get_google(&self) -> Result<Option<GoogleAuth>, String> {
        let s = self.load().map_err(|e| format!("Không đọc được đăng nhập Google: {e}"))?;
        Ok(s.google)
    }

    pub fn set_google(&self, auth: Option<GoogleAuth>) -> Result<(), String> {
        self.modify(|s| s.google = auth)
            .map_err(|e| format!("Không lưu được đăng nhập Google: {e}"))
    }

    fn load(&self) -> Result<Secrets, String> {
        match self.vault.read()? {
            Some(raw) => serde_json::from_str(&raw).map_err(|e| format!("dữ liệu bị hỏng: {e}")),
            None => Ok(Secrets::default()),
        }
    }

    /// Changes one secret and keeps the others; removes the item once empty.
    fn modify(&self, change: impl FnOnce(&mut Secrets)) -> Result<(), String> {
        let mut s = self.load()?;
        change(&mut s);
        if s.api_key.is_none() && s.google.is_none() {
            return self.vault.delete();
        }
        let json = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
        self.vault.write(&json)
    }
}

fn file_vault(config_dir: &Path) -> FileVault {
    FileVault {
        path: config_dir.join("credentials.json"),
    }
}

/// Tightens permissions if someone loosened them.
#[cfg(unix)]
fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode != 0o600 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rl-cred-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An in-memory keychain; `broken` makes every call fail like a locked one.
    #[derive(Clone, Default)]
    struct FakeKeychain {
        item: Arc<Mutex<Option<String>>>,
        broken: bool,
    }

    impl Vault for FakeKeychain {
        fn read(&self) -> Result<Option<String>, String> {
            if self.broken {
                return Err("locked".into());
            }
            Ok(self.item.lock().unwrap().clone())
        }
        fn write(&self, json: &str) -> Result<(), String> {
            if self.broken {
                return Err("locked".into());
            }
            *self.item.lock().unwrap() = Some(json.into());
            Ok(())
        }
        fn delete(&self) -> Result<(), String> {
            *self.item.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn roundtrip_with_private_permissions() {
        let dir = temp_dir("file");
        let creds = Credentials::file_only(&dir);

        assert_eq!(creds.get_api_key().unwrap(), None);
        creds.set_api_key("  abc123 ").unwrap();
        assert_eq!(creds.get_api_key().unwrap().as_deref(), Some("abc123"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join("credentials.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        creds.set_api_key("new").unwrap();
        assert_eq!(creds.get_api_key().unwrap().as_deref(), Some("new"));
        creds.delete_api_key().unwrap();
        assert_eq!(creds.get_api_key().unwrap(), None);
        assert!(!dir.join("credentials.json").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn google_sign_in_and_api_key_are_kept_apart() {
        let dir = temp_dir("google");
        let creds = Credentials::file_only(&dir);
        creds.set_api_key("key").unwrap();
        creds
            .set_google(Some(GoogleAuth {
                client_id: "cid".into(),
                client_secret: "cs".into(),
                refresh_token: "rt".into(),
                user_id: "42".into(),
                email: "a@congty.vn".into(),
            }))
            .unwrap();

        creds.delete_api_key().unwrap();
        assert_eq!(creds.get_google().unwrap().unwrap().refresh_token, "rt");
        creds.set_api_key("key2").unwrap();
        creds.set_google(None).unwrap();
        assert_eq!(creds.get_api_key().unwrap().as_deref(), Some("key2"));
        assert!(creds.get_google().unwrap().is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_old_credentials_file_moves_into_the_keychain() {
        let dir = temp_dir("move");
        Credentials::file_only(&dir).set_api_key("old-key").unwrap();
        let keychain = FakeKeychain::default();

        let creds = Credentials::choose(Some(Box::new(keychain.clone())), file_vault(&dir));
        assert_eq!(creds.storage(), Storage::Keychain);
        assert_eq!(creds.get_api_key().unwrap().as_deref(), Some("old-key"));
        assert!(!dir.join("credentials.json").exists(), "file removed once moved");
        assert!(keychain.item.lock().unwrap().as_deref().unwrap().contains("old-key"));

        creds.delete_api_key().unwrap();
        assert!(keychain.item.lock().unwrap().is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_broken_keychain_keeps_using_the_file() {
        let dir = temp_dir("broken");
        Credentials::file_only(&dir).set_api_key("file-key").unwrap();
        let keychain = FakeKeychain {
            broken: true,
            ..Default::default()
        };

        let creds = Credentials::choose(Some(Box::new(keychain)), file_vault(&dir));
        assert_eq!(creds.storage(), Storage::File);
        assert_eq!(creds.get_api_key().unwrap().as_deref(), Some("file-key"));
        assert!(dir.join("credentials.json").exists());
        let none = Credentials::choose(None, file_vault(&dir));
        assert_eq!(none.storage(), Storage::File);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Touches the real OS keychain with a throwaway item:
    /// `cargo test -p redmine-logtime real_keychain -- --ignored`
    #[test]
    #[ignore]
    fn real_keychain_roundtrip() {
        let entry = keyring::Entry::new("app.redmine-logtime.desktop.test", "roundtrip").unwrap();
        let vault = KeychainVault(entry);
        vault.delete().unwrap();
        assert_eq!(vault.read().unwrap(), None);
        vault.write(r#"{"api_key":"k"}"#).unwrap();
        assert_eq!(vault.read().unwrap().as_deref(), Some(r#"{"api_key":"k"}"#));
        vault.delete().unwrap();
        assert_eq!(vault.read().unwrap(), None);
    }

    #[test]
    fn the_keychain_wins_over_a_leftover_file() {
        let dir = temp_dir("both");
        Credentials::file_only(&dir).set_api_key("stale").unwrap();
        let keychain = FakeKeychain::default();
        *keychain.item.lock().unwrap() = Some(r#"{"api_key":"current"}"#.into());

        let creds = Credentials::choose(Some(Box::new(keychain)), file_vault(&dir));
        assert_eq!(creds.get_api_key().unwrap().as_deref(), Some("current"));
        fs::remove_dir_all(&dir).unwrap();
    }
}

//! Secrets storage: `credentials.json` in the app config dir, readable only by
//! the current user (0600 on macOS/Linux), like `gh` or `aws` CLIs. Holds the
//! Redmine API key and the Google Chat sign-in.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct File {
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

pub struct Credentials {
    path: PathBuf,
}

impl Credentials {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("credentials.json"),
        }
    }

    pub fn get_api_key(&self) -> Result<Option<String>, String> {
        let file = self.load().map_err(|e| format!("Không đọc được API key: {e}"))?;
        Ok(file.api_key.filter(|k| !k.trim().is_empty()))
    }

    pub fn set_api_key(&self, key: &str) -> Result<(), String> {
        self.modify(|f| f.api_key = Some(key.trim().to_string()))
            .map_err(|e| format!("Không lưu được API key: {e}"))
    }

    pub fn delete_api_key(&self) -> Result<(), String> {
        self.modify(|f| f.api_key = None)
            .map_err(|e| format!("Không xoá được API key: {e}"))
    }

    pub fn get_google(&self) -> Result<Option<GoogleAuth>, String> {
        let file = self.load().map_err(|e| format!("Không đọc được đăng nhập Google: {e}"))?;
        Ok(file.google)
    }

    pub fn set_google(&self, auth: Option<GoogleAuth>) -> Result<(), String> {
        self.modify(|f| f.google = auth)
            .map_err(|e| format!("Không lưu được đăng nhập Google: {e}"))
    }

    fn load(&self) -> Result<File, String> {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(File::default()),
            Err(e) => return Err(e.to_string()),
        };
        restrict(&self.path).map_err(|e| format!("không đặt được quyền cho file: {e}"))?;
        serde_json::from_str(&raw).map_err(|e| format!("file bị hỏng: {e}"))
    }

    /// Changes one secret and keeps the others; removes the file once empty.
    fn modify(&self, change: impl FnOnce(&mut File)) -> Result<(), String> {
        let mut file = self.load()?;
        change(&mut file);
        if file.api_key.is_none() && file.google.is_none() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        self.write(&json).map_err(|e| e.to_string())
    }

    /// Writes a private temp file, then renames it over the real one so the
    /// key is never readable by others, even briefly.
    fn write(&self, contents: &str) -> io::Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        let _ = fs::remove_file(&tmp);
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let mut f = opts.open(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, &self.path)
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

    #[test]
    fn roundtrip_with_private_permissions() {
        let dir = std::env::temp_dir().join(format!("rl-cred-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let creds = Credentials::new(&dir);

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
        let dir = std::env::temp_dir().join(format!("rl-cred-g-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let creds = Credentials::new(&dir);
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
}

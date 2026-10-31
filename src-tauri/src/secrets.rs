//! Redmine API key storage: `credentials.json` in the app config dir, readable
//! only by the current user (0600 on macOS/Linux), like `gh` or `aws` CLIs.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    api_key: Option<String>,
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
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Không đọc được API key: {e}")),
        };
        restrict(&self.path).map_err(|e| format!("Không đặt được quyền cho file API key: {e}"))?;
        let file: File =
            serde_json::from_str(&raw).map_err(|e| format!("File API key bị hỏng: {e}"))?;
        Ok(file.api_key.filter(|k| !k.trim().is_empty()))
    }

    pub fn set_api_key(&self, key: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&File {
            api_key: Some(key.trim().to_string()),
        })
        .map_err(|e| e.to_string())?;
        self.write(&json)
            .map_err(|e| format!("Không lưu được API key: {e}"))
    }

    pub fn delete_api_key(&self) -> Result<(), String> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Không xoá được API key: {e}")),
        }
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
        fs::remove_dir_all(&dir).unwrap();
    }
}

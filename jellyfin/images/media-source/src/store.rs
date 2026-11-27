//! 插件数据卷里的两样东西：核心签给本提供方的能力凭据（`credentials issue|revoke`），和每个人的 Jellyfin
//! 令牌（见 [`crate::accounts`]）。文件都是 0600，先写临时文件再改名，进程被杀不会留下半个文件。

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub async fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let path = path.to_path_buf();
    let bytes = bytes.to_vec();
    tokio::task::spawn_blocking(move || write_private_sync(&path, &bytes))
        .await
        .expect("写文件的线程不会 panic")
}

fn write_private_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// 核心签给本提供方的凭据：一个名字一个文件，文件内容就是值。
pub struct Credentials {
    dir: PathBuf,
}

impl Credentials {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(crate::accounts::hex(name.as_bytes()))
    }

    /// 已有同名凭据时不改动它（规范“凭据的签发与吊销”）。
    pub fn issue(&self, name: &str, value: &str) -> std::io::Result<()> {
        if value.is_empty() || value.len() > 4096 {
            return Err(std::io::Error::other("凭据的值不能为空，也不能超过 4 KiB"));
        }
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(name);
        if path.exists() {
            return Ok(());
        }
        write_private_sync(&path, value.as_bytes())
    }

    /// 本来就没有也算成功。
    pub fn revoke(&self, name: &str) -> std::io::Result<()> {
        match std::fs::remove_file(self.path(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// 这个值是不是哪一把已签发的凭据。一把都没有时谁都不认（失败即停，不放行）。
    pub fn accepts(&self, presented: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return false;
        };
        let mut ok = false;
        for entry in entries.flatten() {
            if entry.path().extension().is_some() {
                continue;
            }
            if let Ok(value) = std::fs::read(entry.path()) {
                ok |= constant_time_eq(&value, presented.as_bytes());
            }
        }
        ok
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jf-media-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn nothing_issued_accepts_nothing() {
        let creds = Credentials::new(temp_dir("empty"));
        assert!(!creds.accepts("anything"));
        assert!(!creds.accepts(""));
    }

    #[test]
    fn issue_accept_revoke() {
        let dir = temp_dir("cycle");
        let creds = Credentials::new(dir.clone());
        creds.issue("core", "secret-1").unwrap();
        assert!(creds.accepts("secret-1"));
        assert!(!creds.accepts("secret-2"));
        // 已有同名的不改。
        creds.issue("core", "secret-2").unwrap();
        assert!(creds.accepts("secret-1"));
        creds.revoke("core").unwrap();
        assert!(!creds.accepts("secret-1"));
        creds.revoke("core").unwrap();
        let mode = std::fs::metadata(&dir).map(|m| m.is_dir()).unwrap();
        assert!(mode);
    }

    #[test]
    fn issued_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("mode");
        let creds = Credentials::new(dir.clone());
        creds.issue("core", "v").unwrap();
        let file = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }
}

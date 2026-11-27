//! 核心签给这个适配器的凭据（规范“凭据的签发与吊销”）：`credentials issue <名字>` 从标准输入读值存成文件，
//! `revoke` 删掉。核心每个人一把，请求时放在 `Authorization: Bearer` 里；这里只认“是核心签过的某一把”，
//! 人是谁看 `X-Wonton-User`——凭据名怎么按人起是核心自己的事，适配器不依赖它。

use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use subtle::ConstantTimeEq;

pub struct KeyStore {
    dir: PathBuf,
}

impl KeyStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// 已有同名凭据时不改动它，退出码照样成功；核心发现对不上会先 `revoke` 再 `issue`。
    pub fn issue(&self, name: &str, value: &str) -> io::Result<()> {
        let value = value.trim_end_matches(['\r', '\n']);
        if name.is_empty() || value.is_empty() || value.contains(['\r', '\n']) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "credential name and value must be non-empty, the value a single line",
            ));
        }
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(name);
        if path.exists() {
            return Ok(());
        }
        let staging = self.dir.join(format!(".{}.tmp", file_name(name)));
        write_private(&staging, value)?;
        std::fs::rename(&staging, &path)
    }

    pub fn revoke(&self, name: &str) -> io::Result<()> {
        match std::fs::remove_file(self.path(name)) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    /// 带来的值是不是签过的某一把；目录不存在时谁都不认。
    #[must_use]
    pub fn accepts(&self, presented: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return false;
        };
        let mut matched = false;
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if let Ok(stored) = std::fs::read_to_string(entry.path()) {
                matched |= bool::from(stored.as_bytes().ct_eq(presented.as_bytes()));
            }
        }
        matched
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(file_name(name))
    }
}

/// 凭据名可能带 `/`、`.` 之类的字符，按字节转成十六进制当文件名。
fn file_name(name: &str) -> String {
    use std::fmt::Write;
    name.bytes().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(unix)]
fn write_private(path: &Path, value: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(value.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, value: &str) -> io::Result<()> {
    std::fs::write(path, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_keys_are_accepted_until_revoked_and_reissue_keeps_the_first_value() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyStore::new(dir.path().join("keys"));
        assert!(!keys.accepts("k1"));

        keys.issue("wonton-core.media.alice", "k1\n").unwrap();
        keys.issue("wonton-core.media.alice", "k2").unwrap();
        keys.issue("wonton-core.media.bob/x", "k3").unwrap();
        assert!(keys.accepts("k1"));
        assert!(!keys.accepts("k2"));
        assert!(keys.accepts("k3"));
        assert!(!keys.accepts(""));

        keys.revoke("wonton-core.media.alice").unwrap();
        keys.revoke("wonton-core.media.alice").unwrap();
        assert!(!keys.accepts("k1"));
        assert!(keys.accepts("k3"));
    }

    #[test]
    fn multi_line_values_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyStore::new(dir.path());
        assert!(keys.issue("n", "a\nb").is_err());
        assert!(keys.issue("n", "").is_err());
    }
}

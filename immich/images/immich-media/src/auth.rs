//! 核心签给使用方的能力凭据（插件规范“提供方”一节的 `credentials.issue`/`revoke`）：
//! 值由核心生成、经标准输入给到这里，只存它的 sha256，请求按 `Authorization: Bearer` 比对。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::failure::{Code, Failure};

/// 凭据目录：一个凭据一个文件，文件名是凭据名的 sha256（名字里可能有 `/`），内容是值的 sha256。
pub fn credentials_dir(state: &Path) -> PathBuf {
    state.join("credentials")
}

fn digest(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// `issue`：同名已有就覆盖成这个值（核心在 401 时会先 `revoke` 再 `issue`，值以核心为准）。
pub fn issue(state: &Path, name: &str, value: &str) -> std::io::Result<()> {
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "凭据名和值都不能为空",
        ));
    }
    let dir = credentials_dir(state);
    std::fs::create_dir_all(&dir)?;
    crate::store::write_private(&dir.join(digest(name)), digest(value).as_bytes())
}

/// `revoke`：本来就没有也算成功。
pub fn revoke(state: &Path, name: &str) -> std::io::Result<()> {
    match std::fs::remove_file(credentials_dir(state).join(digest(name))) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

/// 这个 `Authorization` 头是不是核心签过的某个凭据。
pub fn check(state: &Path, header: Option<&str>) -> Result<(), Failure> {
    let denied = |message: &str| Failure::new(Code::AuthenticationRequired, message);
    let token = header
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| denied("缺少能力凭据"))?;
    let wanted = digest(token);
    let entries = match std::fs::read_dir(credentials_dir(state)) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(denied("核心还没签发过凭据"));
        }
        Err(err) => return Err(Failure::io("读凭据目录", &err)),
    };
    for entry in entries {
        let entry = entry.map_err(|err| Failure::io("读凭据目录", &err))?;
        let stored =
            std::fs::read_to_string(entry.path()).map_err(|err| Failure::io("读凭据", &err))?;
        if constant_eq(stored.trim().as_bytes(), wanted.as_bytes()) {
            return Ok(());
        }
    }
    Err(denied("能力凭据不对"))
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_values_pass_and_revoked_ones_do_not() {
        let dir = tempfile::tempdir().unwrap();
        issue(dir.path(), "wonton/media", "secret-1\n").unwrap();
        assert!(check(dir.path(), Some("Bearer secret-1")).is_ok());
        assert_eq!(
            check(dir.path(), Some("Bearer other")).unwrap_err().code,
            Code::AuthenticationRequired
        );
        assert!(check(dir.path(), None).is_err());

        issue(dir.path(), "wonton/media", "secret-2").unwrap();
        assert!(check(dir.path(), Some("Bearer secret-1")).is_err());
        assert!(check(dir.path(), Some("Bearer secret-2")).is_ok());

        revoke(dir.path(), "wonton/media").unwrap();
        revoke(dir.path(), "wonton/media").unwrap();
        assert!(check(dir.path(), Some("Bearer secret-2")).is_err());
    }
}

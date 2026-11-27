//! 家里人 ↔ Jellyfin 用户。一人一个 Jellyfin 用户、一把本人令牌，进度和已看都记在本人名下：家长控制、按人分库和各自的“继续观看”都要靠它。
//!
//! 按 `X-Wonton-User` 找同名用户（不分大小写）；没有就用管理员 key 建一个，口令随机、不保存。本人令牌走
//! Quick Connect 拿：管理员 key 可以替任何用户批准（Jellyfin 12.4 实测），所以全程不碰口令，也不改已有
//! 用户的口令。令牌没有过期时间，存在插件数据卷；Jellyfin 回 401 时丢掉重拿一次。
//!
//! 没选：管理员 key 加 `userId` 参数代办一切。读和写已看可以，但播放会话会记在 API key 名下，没有用户，
//! Jellyfin 自己的“继续观看”和“已看”阈值就不按人算了。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::failure::{Code, Failure, Reply};
use crate::jellyfin::{Identity, Jellyfin, encode};

/// 一个人在 Jellyfin 里的身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub person: String,
    pub user_id: String,
    pub device: String,
    pub token: String,
}

impl Account {
    pub fn identity(&self) -> Identity {
        Identity {
            device: self.device.clone(),
            token: self.token.clone(),
        }
    }
}

pub struct Accounts {
    dir: PathBuf,
    jellyfin: Jellyfin,
    admin: Identity,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct User {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnect {
    code: String,
    secret: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthResult {
    access_token: String,
}

/// 云吞的用户名只含这些字符；别的拒掉，免得拼进文件名和 Jellyfin 用户名出事。
pub fn valid_person(person: &str) -> bool {
    !person.is_empty()
        && person.len() <= 64
        && person
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// 每人一个固定设备号：同一个人重拿令牌时 Jellyfin 覆盖旧令牌，不越积越多；吊销时按它删设备。
pub fn device_for(person: &str) -> String {
    format!("wonton-media-{}", hex(person.as_bytes()))
}

impl Accounts {
    pub fn new(dir: PathBuf, jellyfin: Jellyfin, admin_key: String) -> Self {
        Self {
            dir,
            jellyfin,
            admin: Identity {
                device: "wonton-media-admin".into(),
                token: admin_key,
            },
            locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn admin(&self) -> &Identity {
        &self.admin
    }

    /// 这个人的本人身份；第一次用时建号、拿令牌。
    pub async fn get(&self, person: &str) -> Reply<Account> {
        if !valid_person(person) {
            return Err(Failure::bad_request(format!("认不出这个人：{person:?}")));
        }
        let lock = self.lock_for(person).await;
        let _held = lock.lock().await;
        if let Some(found) = self.load(person).await? {
            return Ok(found);
        }
        let account = self.enroll(person).await?;
        self.save(&account).await?;
        Ok(account)
    }

    /// 本人令牌被 Jellyfin 拒了：丢掉存的那份，下次 [`get`](Self::get) 重拿。
    pub async fn forget(&self, person: &str) -> Reply<()> {
        match tokio::fs::remove_file(self.path(person)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(local(format!("删本人令牌失败：{e}"))),
        }
    }

    /// 收回这个人的上游凭据：删 Jellyfin 里他的设备（令牌随之作废，会话结束），用户和观看记录留着。
    pub async fn revoke(&self, person: &str) -> Reply<()> {
        if !valid_person(person) {
            return Err(Failure::bad_request(format!("认不出这个人：{person:?}")));
        }
        let lock = self.lock_for(person).await;
        let _held = lock.lock().await;
        let path = format!("/Devices?id={}", encode(&device_for(person)));
        match self
            .jellyfin
            .call(Method::DELETE, &path, None, &self.admin, "删这个人的设备")
            .await
        {
            Ok(()) => {}
            // 本来就没有这台设备（没用过或已删过）算成功。
            Err(f) if f.code == Code::MediaGone => {}
            Err(f) => return Err(f),
        }
        self.forget(person).await
    }

    async fn enroll(&self, person: &str) -> Reply<Account> {
        let user_id = match self.find_user(person).await? {
            Some(id) => id,
            None => self.create_user(person).await?,
        };
        let device = device_for(person);
        let token = self.quick_connect(&user_id, &device).await?;
        Ok(Account {
            person: person.to_owned(),
            user_id,
            device,
            token,
        })
    }

    async fn find_user(&self, person: &str) -> Reply<Option<String>> {
        let users: Vec<User> = self
            .jellyfin
            .get("/Users", &self.admin, "列 Jellyfin 用户")
            .await?;
        Ok(users
            .into_iter()
            .find(|u| u.name.eq_ignore_ascii_case(person))
            .map(|u| u.id))
    }

    async fn create_user(&self, person: &str) -> Reply<String> {
        // 口令随机且不保存：本人登录网页走 Quick Connect，管理员也能在 Jellyfin 里重设。
        let password = hex(&random_bytes(24)?);
        let body = json!({"Name": person, "Password": password});
        let user: User = self
            .jellyfin
            .post("/Users/New", &body, &self.admin, "建 Jellyfin 用户")
            .await?;
        Ok(user.id)
    }

    async fn quick_connect(&self, user_id: &str, device: &str) -> Reply<String> {
        let anonymous = Identity {
            device: device.to_owned(),
            token: String::new(),
        };
        let qc: QuickConnect = self
            .jellyfin
            .post("/QuickConnect/Initiate", &Value::Null, &anonymous, "发起 Quick Connect")
            .await
            .map_err(|f| match f.code {
                Code::AuthenticationRequired => Failure::new(
                    Code::AuthenticationRequired,
                    "Jellyfin 关了 Quick Connect：到 控制台 → 服务器 → 常规 打开「在此服务器上启用快速连接」",
                ),
                _ => f,
            })?;
        let authorize = format!(
            "/QuickConnect/Authorize?code={}&userId={}",
            encode(&qc.code),
            encode(user_id)
        );
        self.jellyfin
            .call(
                Method::POST,
                &authorize,
                None,
                &self.admin,
                "批准 Quick Connect",
            )
            .await?;
        let auth: AuthResult = self
            .jellyfin
            .post(
                "/Users/AuthenticateWithQuickConnect",
                &json!({"Secret": qc.secret}),
                &anonymous,
                "用 Quick Connect 换本人令牌",
            )
            .await?;
        Ok(auth.access_token)
    }

    async fn lock_for(&self, person: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.entry(person.to_owned()).or_default().clone()
    }

    fn path(&self, person: &str) -> PathBuf {
        self.dir.join(format!("{}.json", hex(person.as_bytes())))
    }

    async fn load(&self, person: &str) -> Reply<Option<Account>> {
        match tokio::fs::read(self.path(person)).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| local(format!("本人令牌文件坏了：{e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(local(format!("读本人令牌失败：{e}"))),
        }
    }

    async fn save(&self, account: &Account) -> Reply<()> {
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| local(format!("建令牌目录失败：{e}")))?;
        let bytes = serde_json::to_vec(account).expect("Account 总能序列化");
        crate::store::write_private(&self.path(&account.person), &bytes)
            .await
            .map_err(|e| local(format!("存本人令牌失败：{e}")))
    }
}

/// 适配器自己的数据卷读写失败：插件这一侧坏了。
fn local(message: String) -> Failure {
    Failure::new(Code::PluginUnavailable, message)
}

fn random_bytes(n: usize) -> Reply<Vec<u8>> {
    let mut buf = vec![0u8; n];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .map_err(|e| local(format!("读随机数失败：{e}")))?;
    Ok(buf)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn person_names_are_restricted() {
        assert!(valid_person("mom"));
        assert!(valid_person("li.si-2"));
        assert!(!valid_person(""));
        assert!(!valid_person("../etc"));
        assert!(!valid_person("a b"));
    }

    #[test]
    fn device_is_stable_per_person() {
        assert_eq!(device_for("mom"), device_for("mom"));
        assert_ne!(device_for("mom"), device_for("dad"));
    }
}

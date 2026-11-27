//! 每个家人自己的 Immich 账号和 API key。
//!
//! 第一次替某人办事时建号，和网页 OIDC 自动开户同一个时机：核心只把授权打开本插件的人转过来。
//! 邮箱沿用网页那条的约定（主人用注入的邮箱，家人用 `<用户名>@wonton.internal`），之后他用 OIDC 登录网页，
//! Immich 按邮箱接上同一个账号。
//!
//! Immich 的管理员不能替别人建 API key（key 只能由本人的会话建），所以开户是：管理员会话建号或给已有的号
//! 设一个随机密码 → 用这个密码登录 → 建 key。家人网页走 OIDC，这个密码不给人用；主人的密码就是安装时
//! 生成的 `admin_password`（局域网直连在用），不改它。

use std::path::{Path, PathBuf};

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::failure::{Code, Failure, Result};
use crate::immich::{Auth, Client};
use crate::store;

/// 适配器要的权限：浏览、取字节、上传、收藏和回收站、相册与共享、人物、按 id 认人。
/// 不含 `library.*`、`adminUser.*` 这类管理员的事。
pub const KEY_PERMISSIONS: &[&str] = &[
    "asset.read",
    "asset.view",
    "asset.download",
    "asset.upload",
    "asset.update",
    "asset.delete",
    "timeline.read",
    "album.read",
    "album.create",
    "album.update",
    "albumAsset.create",
    "albumAsset.delete",
    "albumUser.create",
    "person.read",
    "user.read",
];

const KEY_NAME: &str = "wonton-media";

#[derive(Debug, Clone)]
pub struct Owner {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub user: String,
    pub user_id: String,
    pub email: String,
    pub api_key: String,
}

pub struct Accounts {
    dir: PathBuf,
    owner: Owner,
    client: Client,
    // 同一时刻只开一个户：两个请求同时替新人办事时，第二个等第一个建完再读文件。
    opening: Mutex<()>,
}

/// `X-Wonton-User` 是 Matrix ID（`@alice:example.org`），本机用户名是它的 localpart。
pub fn username(header: Option<&str>) -> Result<String> {
    let raw = header
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| Failure::protocol("缺少 X-Wonton-User"))?;
    let local = match raw.strip_prefix('@') {
        Some(rest) => rest.split_once(':').map(|(local, _)| local).unwrap_or(rest),
        None => raw,
    };
    let valid = !local.is_empty()
        && local
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._=-".contains(&b));
    if !valid {
        return Err(Failure::protocol(format!(
            "X-Wonton-User 不是本机用户：{raw}"
        )));
    }
    Ok(local.to_owned())
}

impl Accounts {
    pub fn new(state: &Path, owner: Owner, client: Client) -> Self {
        Self {
            dir: state.join("accounts"),
            owner,
            client,
            opening: Mutex::new(()),
        }
    }

    pub fn email_of(&self, user: &str) -> String {
        if user == self.owner.name {
            self.owner.email.clone()
        } else {
            format!("{user}@wonton.internal")
        }
    }

    /// 反查：Immich 账号的邮箱对应哪个本机用户；对不上（Immich 后台手工建的号）就没有。
    pub fn user_of_email(&self, email: &str) -> Option<String> {
        if email.eq_ignore_ascii_case(&self.owner.email) {
            return Some(self.owner.name.clone());
        }
        email
            .to_ascii_lowercase()
            .strip_suffix("@wonton.internal")
            .map(str::to_owned)
    }

    fn path(&self, user: &str) -> PathBuf {
        self.dir.join(format!("{user}.json"))
    }

    fn read(&self, user: &str) -> Result<Option<Account>> {
        match std::fs::read(self.path(user)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|err| Failure::upstream(format!("账号文件坏了（{user}）：{err}"))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Failure::io("读账号文件", &err)),
        }
    }

    /// 这个人的账号；没有就开户。
    pub async fn get(&self, user: &str) -> Result<Account> {
        if let Some(account) = self.read(user)? {
            return Ok(account);
        }
        let _guard = self.opening.lock().await;
        if let Some(account) = self.read(user)? {
            return Ok(account);
        }
        let account = self.open(user).await?;
        std::fs::create_dir_all(&self.dir).map_err(|err| Failure::io("建账号目录", &err))?;
        let bytes = serde_json::to_vec(&account).expect("账号能序列化");
        store::write_private(&self.path(user), &bytes)
            .map_err(|err| Failure::io("写账号文件", &err))?;
        Ok(account)
    }

    /// Immich 说这把 key 不认了（用户在 Immich 里删了它）：丢掉记录，下一次请求重新开。
    pub fn forget(&self, user: &str) {
        if let Err(err) = std::fs::remove_file(self.path(user))
            && err.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("删账号文件失败（{user}）：{err}");
        }
    }

    async fn open(&self, user: &str) -> Result<Account> {
        let email = self.email_of(user);
        let password = if user == self.owner.name {
            self.owner.password.clone()
        } else {
            let admin = self
                .client
                .login(&self.owner.email, &self.owner.password)
                .await
                .map_err(|err| admin_failed(&err))?;
            let result = self.set_member_password(&admin, user, &email).await;
            self.client.logout(&admin).await;
            result?
        };
        let session = self.client.login(&email, &password).await?;
        let result = self.create_key(&session).await;
        self.client.logout(&session).await;
        let (user_id, api_key) = result?;
        Ok(Account {
            user: user.to_owned(),
            user_id,
            email,
            api_key,
        })
    }

    /// 家人的号：没有就建，有就换一个随机密码（网页走 OIDC 用不到它）。
    async fn set_member_password(&self, admin: &str, user: &str, email: &str) -> Result<String> {
        let password = store::random_hex(24);
        let users: Vec<Value> = self
            .client
            .json(Method::GET, "/admin/users", Auth::Session(admin), None)
            .await?;
        let existing = users.iter().find(|u| {
            u["email"]
                .as_str()
                .is_some_and(|e| e.eq_ignore_ascii_case(email))
        });
        let body = match existing {
            Some(found) => {
                let id = found["id"]
                    .as_str()
                    .ok_or_else(|| Failure::protocol("Immich 用户没有 id"))?;
                let body = json!({"password": password, "shouldChangePassword": false});
                let path = format!("/admin/users/{id}");
                self.client
                    .json::<Value>(Method::PUT, &path, Auth::Session(admin), Some(&body))
                    .await?;
                return Ok(password);
            }
            None => json!({
                "email": email,
                "name": user,
                "password": password,
                "shouldChangePassword": false,
            }),
        };
        self.client
            .json::<Value>(
                Method::POST,
                "/admin/users",
                Auth::Session(admin),
                Some(&body),
            )
            .await?;
        Ok(password)
    }

    async fn create_key(&self, session: &str) -> Result<(String, String)> {
        let body = json!({"name": KEY_NAME, "permissions": KEY_PERMISSIONS});
        let created: Value = self
            .client
            .json(
                Method::POST,
                "/api-keys",
                Auth::Session(session),
                Some(&body),
            )
            .await?;
        let secret = created["secret"]
            .as_str()
            .ok_or_else(|| Failure::protocol("Immich 建 key 的回包里没有 secret"))?;
        let me: Value = self
            .client
            .json(Method::GET, "/users/me", Auth::Session(session), None)
            .await?;
        let id = me["id"]
            .as_str()
            .ok_or_else(|| Failure::protocol("Immich /users/me 没有 id"))?;
        Ok((id.to_owned(), secret.to_owned()))
    }
}

fn admin_failed(err: &Failure) -> Failure {
    match err.code {
        Code::AuthenticationRequired => Failure::upstream(format!(
            "Immich 管理员登录失败，开不了户（{}）；管理员的密码在 Immich 里被改过时会这样",
            err.message
        )),
        _ => err.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_come_from_matrix_localparts() {
        assert_eq!(username(Some("@alice:example.org")).unwrap(), "alice");
        assert_eq!(username(Some("bob")).unwrap(), "bob");
        for bad in [None, Some(""), Some("@:x"), Some("@Al ice:x"), Some("../x")] {
            assert_eq!(username(bad).unwrap_err().code, Code::Protocol, "{bad:?}");
        }
    }
}

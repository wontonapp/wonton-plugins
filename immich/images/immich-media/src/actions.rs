//! 原生页上本人点的写操作（契约的 `LibraryAction`）翻成 Immich 的接口。不认识的动作在反序列化时就回
//! `protocol`；部分成功的逐条回 `failed`。

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::App;
use crate::accounts::Account;
use crate::failure::{Code, Failure, Result};
use crate::library::uuid;
use crate::library_map;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum LibraryAction {
    Favorite {
        ids: Vec<String>,
        favorite: bool,
    },
    CreateAlbum {
        title: String,
        #[serde(default)]
        ids: Vec<String>,
        #[serde(default)]
        share_with: Vec<String>,
    },
    AddToAlbum {
        album: String,
        ids: Vec<String>,
    },
    RemoveFromAlbum {
        album: String,
        ids: Vec<String>,
    },
    ShareAlbum {
        album: String,
        people: Vec<String>,
    },
    Trash {
        ids: Vec<String>,
    },
    Restore {
        ids: Vec<String>,
    },
}

fn ids(list: &[String]) -> Result<Vec<&str>> {
    if list.is_empty() {
        return Err(Failure::protocol("至少要一条"));
    }
    list.iter().map(|id| uuid(id)).collect()
}

/// 家人的本机用户名 → Immich 用户 id。对不上的人直接报错，不悄悄少共享一个。
async fn user_ids(app: &App, account: &Account, people: &[String]) -> Result<Vec<String>> {
    let users = app.users(account).await?;
    people
        .iter()
        .map(|name| {
            users
                .iter()
                .find(|(_, user)| *user == name)
                .map(|(id, _)| id.clone())
                .ok_or_else(|| {
                    Failure::new(
                        Code::PermissionDenied,
                        format!("{name} 还没在相册里开过户，先让他打开一次相册"),
                    )
                })
        })
        .collect()
}

/// Immich 的 `BulkIdResponseDto[]` → 成了几条、没成的逐条原因。
fn bulk(reply: &Value) -> Result<(usize, Vec<Value>)> {
    let list = reply
        .as_array()
        .ok_or_else(|| Failure::protocol("Immich 的批量回包不是数组"))?;
    let mut failed = Vec::new();
    for entry in list {
        if entry["success"].as_bool() == Some(true) {
            continue;
        }
        let code = match entry["error"].as_str() {
            Some("no_permission") => "permission_denied",
            Some("not_found") => "media_gone",
            // 已经在相册里了：结果和用户要的一样，不算失败。
            Some("duplicate") => continue,
            _ => "upstream_error",
        };
        failed.push(json!({
            "id": entry["id"],
            "failure": {"code": code, "message": entry["errorMessage"].as_str().unwrap_or(code)},
        }));
    }
    Ok((list.len() - failed.len(), failed))
}

async fn album_value(app: &App, account: &Account, reply: &Value) -> Result<Value> {
    let users = app.users(account).await?;
    library_map::album(reply, &account.user_id, &users, app.household.as_deref())
}

pub async fn run(app: &App, account: &Account, action: LibraryAction) -> Result<Value> {
    match action {
        LibraryAction::Favorite {
            ids: list,
            favorite,
        } => {
            let list = ids(&list)?;
            let body = json!({"ids": list, "isFavorite": favorite});
            app.call::<Value>(account, Method::PUT, "/assets", Some(&body))
                .await?;
            Ok(json!({"changed": list.len()}))
        }
        LibraryAction::CreateAlbum {
            title,
            ids: list,
            share_with,
        } => {
            if title.trim().is_empty() {
                return Err(Failure::protocol("相册要有名字"));
            }
            let assets: Vec<&str> = list.iter().map(|id| uuid(id)).collect::<Result<_>>()?;
            let members = user_ids(app, account, &share_with).await?;
            let album_users: Vec<Value> = members
                .iter()
                .map(|id| json!({"userId": id, "role": "editor"}))
                .collect();
            let body = json!({"albumName": title, "assetIds": assets, "albumUsers": album_users});
            let reply: Value = app
                .call(account, Method::POST, "/albums", Some(&body))
                .await?;
            let album = album_value(app, account, &reply).await?;
            Ok(json!({"changed": assets.len(), "album": album}))
        }
        LibraryAction::AddToAlbum { album, ids: list } => {
            album_assets(app, account, &album, &list, Method::PUT).await
        }
        LibraryAction::RemoveFromAlbum { album, ids: list } => {
            album_assets(app, account, &album, &list, Method::DELETE).await
        }
        LibraryAction::ShareAlbum { album, people } => {
            let album = uuid(&album)?;
            let members = user_ids(app, account, &people).await?;
            let album_users: Vec<Value> = members
                .iter()
                .map(|id| json!({"userId": id, "role": "editor"}))
                .collect();
            let body = json!({"albumUsers": album_users});
            let path = format!("/albums/{album}/users");
            let reply: Value = app.call(account, Method::PUT, &path, Some(&body)).await?;
            let album = album_value(app, account, &reply).await?;
            Ok(json!({"changed": members.len(), "album": album}))
        }
        LibraryAction::Trash { ids: list } => {
            let list = ids(&list)?;
            let body = json!({"ids": list, "force": false});
            app.call::<Value>(account, Method::DELETE, "/assets", Some(&body))
                .await?;
            Ok(json!({"changed": list.len()}))
        }
        LibraryAction::Restore { ids: list } => {
            let list = ids(&list)?;
            let body = json!({"ids": list});
            let reply: Value = app
                .call(account, Method::POST, "/trash/restore/assets", Some(&body))
                .await?;
            Ok(json!({"changed": reply["count"].as_u64().unwrap_or(0)}))
        }
    }
}

async fn album_assets(
    app: &App,
    account: &Account,
    album: &str,
    list: &[String],
    method: Method,
) -> Result<Value> {
    let album = uuid(album)?;
    let body = json!({"ids": ids(list)?});
    let path = format!("/albums/{album}/assets");
    let reply: Value = app.call(account, method, &path, Some(&body)).await?;
    let (changed, failed) = bulk(&reply)?;
    let mut out = json!({"changed": changed});
    if !failed.is_empty() {
        out["failed"] = failed.into();
    }
    Ok(out)
}

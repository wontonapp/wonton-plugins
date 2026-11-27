//! Immich 的回包翻成契约的相册形状（`wonton-protocol::media::library`）。只做字段映射，不补数据：
//! Immich 没给的就空着。

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::failure::{Failure, Result};

/// 这个人能看到的 Immich 用户，翻成家人的 Matrix ID（同一台服务器上）。
#[derive(Debug, Default)]
pub struct Users {
    /// Immich 用户 id → Matrix ID，只有认得回家人的号。对不上的（Immich 后台手工建的号）不在这里：它的条目
    /// 和相册标成主人不明，它不进成员名单，也共享不到它；它的邮箱、名字和 Immich id 都不往外写
    pub ids: HashMap<String, String>,
    /// 家庭相册库主的 Matrix ID；没设就不单列家庭相册
    pub household: Option<String>,
}

impl Users {
    fn matrix_id(&self, immich_id: &str) -> Option<&str> {
        self.ids.get(immich_id).map(String::as_str)
    }

    /// 反查：这个 Matrix ID 在 Immich 里的用户 id。
    pub fn immich_id(&self, matrix_id: &str) -> Option<&str> {
        self.ids
            .iter()
            .find(|(_, name)| *name == matrix_id)
            .map(|(id, _)| id.as_str())
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PhotoAsset {
    pub id: String,
    pub kind: &'static str,
    pub taken_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub live: bool,
    pub favorite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub owner_unknown: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbhash: Option<String>,
}

/// 条目的主人：本人的什么都不写；家人的写 Matrix ID；对不上家人的只标主人不明。
#[derive(Debug, PartialEq)]
enum Owner {
    Me,
    Person(String),
    Unknown,
}

impl Owner {
    fn of(owner_id: Option<&str>, me: &str, users: &Users) -> Self {
        match owner_id {
            None => Self::Me,
            Some(id) if id == me => Self::Me,
            Some(id) => users
                .matrix_id(id)
                .map_or(Self::Unknown, |name| Self::Person(name.to_owned())),
        }
    }

    fn fields(self) -> (Option<String>, bool) {
        match self {
            Self::Me => (None, false),
            Self::Person(name) => (Some(name), false),
            Self::Unknown => (None, true),
        }
    }
}

pub fn millis(text: &str) -> Result<u64> {
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|at| at.timestamp_millis().max(0) as u64)
        .map_err(|err| Failure::protocol(format!("Immich 的时间看不懂：{text}：{err}")))
}

fn column<'a>(bucket: &'a Value, name: &str, len: usize) -> Result<&'a [Value]> {
    match bucket[name].as_array() {
        Some(values) if values.len() == len => Ok(values),
        Some(_) => Err(Failure::protocol(format!(
            "Immich 时间线的 {name} 列长度对不上"
        ))),
        None => Err(Failure::protocol(format!("Immich 时间线缺 {name} 列"))),
    }
}

/// `GET /timeline/bucket` 是按列给的（每个字段一个等长数组），翻成一行一条。
pub fn bucket_assets(bucket: &Value, me: &str, users: &Users) -> Result<Vec<PhotoAsset>> {
    let ids = bucket["id"]
        .as_array()
        .ok_or_else(|| Failure::protocol("Immich 时间线缺 id 列"))?;
    let len = ids.len();
    let image = column(bucket, "isImage", len)?;
    let created = column(bucket, "fileCreatedAt", len)?;
    let ratio = column(bucket, "ratio", len)?;
    let duration = column(bucket, "duration", len)?;
    let live = column(bucket, "livePhotoVideoId", len)?;
    let favorite = column(bucket, "isFavorite", len)?;
    let owners = column(bucket, "ownerId", len)?;
    let thumbhash = column(bucket, "thumbhash", len)?;
    let mut assets = Vec::with_capacity(len);
    for i in 0..len {
        let id = ids[i]
            .as_str()
            .ok_or_else(|| Failure::protocol("Immich 时间线的 id 不是字符串"))?;
        let created = created[i]
            .as_str()
            .ok_or_else(|| Failure::protocol("Immich 时间线的 fileCreatedAt 不是字符串"))?;
        let (owner, owner_unknown) = Owner::of(owners[i].as_str(), me, users).fields();
        assets.push(PhotoAsset {
            id: id.to_owned(),
            kind: if image[i].as_bool().unwrap_or(true) {
                "photo"
            } else {
                "video"
            },
            taken_at: millis(created)?,
            aspect: ratio[i].as_f64().filter(|r| *r > 0.0),
            duration_ms: duration[i].as_u64(),
            live: live[i].is_string(),
            favorite: favorite[i].as_bool().unwrap_or(false),
            owner,
            owner_unknown,
            thumbhash: thumbhash[i].as_str().map(str::to_owned),
        });
    }
    Ok(assets)
}

/// 一条 `AssetResponseDto`（搜索、相册、详情里的）。
pub fn asset(value: &Value, me: &str, users: &Users) -> Result<PhotoAsset> {
    let id = value["id"]
        .as_str()
        .ok_or_else(|| Failure::protocol("Immich 的条目没有 id"))?;
    let created = value["fileCreatedAt"]
        .as_str()
        .ok_or_else(|| Failure::protocol("Immich 的条目没有 fileCreatedAt"))?;
    let (width, height) = (value["width"].as_f64(), value["height"].as_f64());
    let (owner, owner_unknown) = Owner::of(value["ownerId"].as_str(), me, users).fields();
    Ok(PhotoAsset {
        id: id.to_owned(),
        kind: if value["type"] == "VIDEO" {
            "video"
        } else {
            "photo"
        },
        taken_at: millis(created)?,
        aspect: match (width, height) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Some(w / h),
            _ => None,
        },
        duration_ms: value["duration"].as_u64(),
        live: value["livePhotoVideoId"].is_string(),
        favorite: value["isFavorite"].as_bool().unwrap_or(false),
        owner,
        owner_unknown,
        thumbhash: value["thumbhash"].as_str().map(str::to_owned),
    })
}

/// 大图页的详情。
pub fn detail(value: &Value, me: &str, users: &Users) -> Result<Value> {
    let exif = &value["exifInfo"];
    let text = |v: &Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let camera = match (text(&exif["make"]), text(&exif["model"])) {
        (Some(make), Some(model)) if model.starts_with(&make) => Some(model),
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (make, model) => model.or(make),
    };
    let place: Vec<String> = ["city", "state", "country"]
        .iter()
        .filter_map(|key| text(&exif[*key]))
        .collect();
    let location = match (exif["latitude"].as_f64(), exif["longitude"].as_f64()) {
        (Some(lat), Some(lon)) => Some(json!({"lat": lat, "lon": lon})),
        _ => None,
    };
    let people: Vec<Value> = value["people"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["name"].as_str().is_some_and(|n| !n.is_empty()))
        .map(|p| json!({"id": p["id"], "name": p["name"]}))
        .collect();
    let fields: [(&str, Option<Value>); 10] = [
        (
            "width",
            exif["exifImageWidth"]
                .as_u64()
                .or(value["width"].as_u64())
                .map(Value::from),
        ),
        (
            "height",
            exif["exifImageHeight"]
                .as_u64()
                .or(value["height"].as_u64())
                .map(Value::from),
        ),
        (
            "file_name",
            text(&value["originalFileName"]).map(Value::from),
        ),
        ("size", exif["fileSizeInByte"].as_u64().map(Value::from)),
        ("description", text(&exif["description"]).map(Value::from)),
        ("camera", camera.map(Value::from)),
        ("lens", text(&exif["lensModel"]).map(Value::from)),
        ("place", (!place.is_empty()).then(|| place.into())),
        ("location", location),
        ("people", (!people.is_empty()).then(|| people.into())),
    ];
    let mut out = json!({ "asset": asset(value, me, users)? });
    for (key, field) in fields {
        if let Some(field) = field {
            out[key] = field;
        }
    }
    Ok(out)
}

/// 一个相册：`albumUsers` 第一项总是库主（Immich OpenAPI 的说明）。
pub fn album(value: &Value, me: &str, users: &Users) -> Result<Value> {
    let id = value["id"]
        .as_str()
        .ok_or_else(|| Failure::protocol("Immich 的相册没有 id"))?;
    let members: Vec<&str> = value["albumUsers"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|u| u["user"]["id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let owner_id = *members
        .first()
        .ok_or_else(|| Failure::protocol("Immich 的相册没有库主"))?;
    let owner = match Owner::of(Some(owner_id), me, users) {
        Owner::Me => "me".to_owned(),
        Owner::Person(name) if Some(name.as_str()) == users.household.as_deref() => {
            "household".to_owned()
        }
        Owner::Person(name) => format!("person:{name}"),
        Owner::Unknown => "unknown".to_owned(),
    };
    let names: Vec<&str> = members[1..]
        .iter()
        .filter_map(|id| users.matrix_id(id))
        .collect();
    let mut out = json!({
        "id": id,
        "title": value["albumName"].as_str().unwrap_or_default(),
        "count": value["assetCount"].as_u64().unwrap_or(0),
        "owner": owner,
        "shared": value["shared"].as_bool().unwrap_or(false),
    });
    if let Some(cover) = value["albumThumbnailAssetId"].as_str() {
        out["cover"] = cover.into();
    }
    if !names.is_empty() {
        out["members"] = names.into();
    }
    if let Some(updated) = value["updatedAt"].as_str() {
        out["updated_at"] = millis(updated)?.into();
    }
    Ok(out)
}

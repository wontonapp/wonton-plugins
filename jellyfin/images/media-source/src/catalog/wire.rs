//! 影视浏览回包的线上形状，照主仓 `wonton-protocol::media::catalog` 抄一份（本仓库不链主仓的 crate）。
//! 只出不进：适配器只序列化；字段名和省略规则以主仓为准，改了两边一起改。

use serde::{Deserialize, Serialize};

use crate::contract::MediaTrack;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogLibrary {
    pub id: String,
    pub title: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Libraries {
    pub libraries: Vec<CatalogLibrary>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct CatalogEntry {
    pub id: String,
    pub entry_type: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_ms: Option<u64>,
    pub played: bool,
    pub favorite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unplayed_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_rating: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_rating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_at: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub artwork: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntryPage {
    pub entries: Vec<CatalogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Credit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    pub artwork: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct MediaInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub hdr: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_channels: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct EntryDetail {
    pub entry: CatalogEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub studios: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premiered_at: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<Credit>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CatalogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_up: Option<CatalogEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<MediaTrack>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<MediaTrack>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Genres {
    pub genres: Vec<String>,
}

/// 收的体：认不得的 `kind` 解析失败，按协议错回（不静默忽略）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CatalogAction {
    SetPlayed { ids: Vec<String>, played: bool },
    Favorite { ids: Vec<String>, favorite: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionFailure {
    pub id: String,
    pub failure: crate::failure::Failure,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogActionResult {
    pub changed: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<ActionFailure>,
}

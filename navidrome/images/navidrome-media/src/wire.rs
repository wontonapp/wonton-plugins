//! 插件规范“媒体源：路径”里的请求体和响应体，只写这个适配器读写的字段；多出来的字段忽略。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaItemRef {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_ms: Option<u64>,
    pub series: Option<String>,
    pub index: Option<u32>,
    pub played: bool,
    pub starred: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemsPage {
    pub items: Vec<MediaItemRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ItemsQuery {
    pub kind: Option<String>,
    pub q: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub cursor: Option<String>,
}

/// 只读条目编号；`PlaybackRequest` 里条目的其余字段由核心带来，播放计划以上游为准。
#[derive(Debug, Deserialize)]
pub struct ItemId {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceProfile {
    #[serde(default)]
    pub direct_play: Vec<String>,
    /// 比特每秒，0 是不限。
    #[serde(default)]
    pub max_bitrate: u32,
    #[serde(default)]
    pub container: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlaybackRequest {
    pub item: ItemId,
    pub profile: DeviceProfile,
}

/// `ProviderStream`：单键对象，`{"direct": 地址}` 或 `{"transcoded": 地址}`。地址是相对本适配器的路径，
/// 核心带着同样的凭据和 `X-Wonton-User` 来取。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStream {
    Direct(String),
    Transcoded(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaTrack {
    pub index: u16,
    pub language: String,
    pub label: Option<String>,
    pub kind: String,
    pub selected: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaybackPlan {
    pub session: String,
    pub stream: ProviderStream,
    pub origin_stream: Option<ProviderStream>,
    pub segments: Vec<serde_json::Value>,
    pub audio: Vec<MediaTrack>,
    pub subtitles: Vec<MediaTrack>,
    pub seekable: bool,
    pub duration_ms: u64,
    pub expires_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct PlaybackProgress {
    pub position_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct QueueSnapshot {
    pub entries: Vec<ItemId>,
    pub current: Option<usize>,
    #[serde(default)]
    pub position_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct PlaybackStopped {
    pub position_ms: u64,
    /// `StopReason`：认识的是字符串，新版本可能带载荷，所以不限定形状。
    pub reason: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub operations: &'static [&'static str],
    pub kinds: &'static [&'static str],
}

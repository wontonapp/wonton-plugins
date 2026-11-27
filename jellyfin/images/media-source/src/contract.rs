//! 媒体源契约的 JSON 形状（规范“媒体源：路径”、`wonton-protocol` 的 `media` 模块）。适配器是独立组件，
//! 照着契约写自己的结构，不链接核心的 crate；字段名和取值与协议一一对应，测试按 JSON 断言。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaItemRef {
    pub id: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(default)]
    pub played: bool,
    #[serde(default)]
    pub starred: bool,
}

/// 设备能力档。`direct_play` 是 MIME，编解码写在 `codecs` 参数里（`video/mp4; codecs="avc1,mp4a"`），
/// 也收不带参数的编解码名（`h264`）；`container` 是能直接放的容器扩展名。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct DeviceProfile {
    #[serde(default)]
    pub direct_play: Vec<String>,
    #[serde(default)]
    pub max_bitrate: u32,
    #[serde(default)]
    pub hdr: bool,
    #[serde(default)]
    pub container: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlaybackRequest {
    #[serde(default)]
    pub position_ms: u64,
    #[serde(default)]
    pub audio: Option<u16>,
    #[serde(default)]
    pub subtitle: Option<u16>,
    pub item: MediaItemRef,
    #[serde(default)]
    pub profile: DeviceProfile,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStream {
    Direct(String),
    Transcoded(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaTrack {
    pub index: u16,
    pub language: String,
    pub label: Option<String>,
    pub kind: &'static str,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    pub kind: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaybackPlan {
    pub session: String,
    pub stream: ProviderStream,
    pub origin_stream: Option<ProviderStream>,
    pub segments: Vec<Segment>,
    pub audio: Vec<MediaTrack>,
    pub subtitles: Vec<MediaTrack>,
    pub seekable: bool,
    pub duration_ms: u64,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Progress {
    pub position_ms: u64,
    /// 规范只要位置和墙钟；壳知道是否暂停时一并带上，Jellyfin 的“正在播放”据此显示。
    #[serde(default)]
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Stopped {
    pub position_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Revoke {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemsPage {
    pub items: Vec<MediaItemRef>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Capabilities {
    pub operations: &'static [&'static str],
    pub kinds: &'static [&'static str],
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stream_is_an_externally_tagged_lowercase_object() {
        let direct = serde_json::to_value(ProviderStream::Direct("/a".into())).unwrap();
        assert_eq!(direct, json!({"direct": "/a"}));
        let hls = serde_json::to_value(ProviderStream::Transcoded("/b".into())).unwrap();
        assert_eq!(hls, json!({"transcoded": "/b"}));
    }

    #[test]
    fn playback_request_reads_the_protocol_shape() {
        let body = json!({
            "position_ms": 1500, "audio": 2, "subtitle": null,
            "item": {"id": "x", "kind": "video", "title": "T", "artist": null, "album": null,
                     "artwork_url": null, "duration_ms": 30000, "series": null, "index": null,
                     "played": false, "starred": false},
            "device": "phone-1", "person": "mom",
            "profile": {"direct_play": ["video/mp4; codecs=\"avc1,mp4a\""], "max_bitrate": 8000000,
                        "hdr": false, "container": ["mp4"]}
        });
        let req: PlaybackRequest = serde_json::from_value(body).unwrap();
        assert_eq!(req.position_ms, 1500);
        assert_eq!(req.audio, Some(2));
        assert_eq!(req.item.id, "x");
        assert_eq!(req.profile.max_bitrate, 8_000_000);
    }
}

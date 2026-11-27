//! 播放：`POST /playback` 出计划，进度、停止、队列翻成 `scrobble` 和 `savePlayQueue`。会话号就是歌的编号，
//! 适配器不存会话。

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::failure::{MediaFailure, Reply};
use crate::items::segment;
use crate::upstream::Upstream;
use crate::wire::{
    DeviceProfile, MediaTrack, PlaybackPlan, PlaybackRequest, PlaybackStopped, ProviderStream,
    QueueSnapshot,
};

/// 流地址是适配器自己的路径，不会失效；给一个宽裕的过期时间，核心到点重取计划。
const PLAN_LIFETIME_MS: u64 = 24 * 60 * 60 * 1000;
/// 转码的码率上限（kbps）：设备没限或限得更高时用它。
const TRANSCODE_CEILING_KBPS: u32 = 320;
const TRANSCODE_FLOOR_KBPS: u32 = 32;
/// 转码可选的格式：Navidrome 自带的转码配置名、设备档里认它的 MIME 和容器名。按顺序挑第一个设备收的。
const TRANSCODES: [(&str, &[&str], &[&str]); 3] = [
    ("mp3", &["audio/mpeg", "audio/mp3"], &["mp3"]),
    ("aac", &["audio/aac", "audio/mp4"], &["aac", "m4a"]),
    ("opus", &["audio/ogg", "audio/opus"], &["opus", "ogg"]),
];
/// 记一次“听过”的门槛，照 Last.fm 和多数 Subsonic 客户端：放完，或放过一半，或放过四分钟。
const SCROBBLE_AFTER_MS: u64 = 4 * 60 * 1000;

pub(crate) async fn plan(
    upstream: &Upstream,
    person: &str,
    request: &PlaybackRequest,
) -> Reply<PlaybackPlan> {
    let song = song(upstream, person, &request.item.id).await?;
    let text = |field: &str| song.get(field).and_then(Value::as_str).unwrap_or("");
    let kbps = song.get("bitRate").and_then(Value::as_u64).unwrap_or(0);
    let profile = &request.profile;
    let fits = profile.max_bitrate == 0 || kbps * 1000 <= u64::from(profile.max_bitrate);
    let path = format!("/items/{}/stream", segment(&request.item.id));
    let direct = fits && accepts(profile, &[text("contentType")], &[text("suffix")]);
    let stream = if direct {
        ProviderStream::Direct(path)
    } else {
        let (format, _, _) = TRANSCODES
            .iter()
            .find(|(_, types, containers)| accepts(profile, types, containers))
            .ok_or_else(|| {
                MediaFailure::new(
                    "unsupported_by_device",
                    format!(
                        "这台设备不收 {}，也不收能转码成的 mp3、aac、opus",
                        text("suffix")
                    ),
                )
            })?;
        let limit = match profile.max_bitrate / 1000 {
            0 => TRANSCODE_CEILING_KBPS,
            limit => limit.clamp(TRANSCODE_FLOOR_KBPS, TRANSCODE_CEILING_KBPS),
        };
        ProviderStream::Transcoded(format!("{path}?format={format}&max_bitrate={limit}"))
    };
    Ok(PlaybackPlan {
        session: request.item.id.clone(),
        seekable: matches!(stream, ProviderStream::Direct(_)),
        stream,
        origin_stream: None,
        segments: vec![],
        audio: vec![MediaTrack {
            index: 0,
            language: "und".to_owned(),
            label: None,
            kind: "audio".to_owned(),
            selected: true,
        }],
        subtitles: vec![],
        duration_ms: song.get("duration").and_then(Value::as_u64).unwrap_or(0) * 1000,
        expires_at: now_ms() + PLAN_LIFETIME_MS,
    })
}

/// 进度：告诉 Navidrome 正在放（“正在播放”列表），不计播放次数。
pub(crate) async fn progress(upstream: &Upstream, person: &str, session: &str) -> Reply<()> {
    let params = vec![
        ("id", session.to_owned()),
        ("submission", "false".to_owned()),
    ];
    upstream.call(person, "scrobble", &params).await.map(|_| ())
}

pub(crate) async fn stopped(
    upstream: &Upstream,
    person: &str,
    session: &str,
    stop: &PlaybackStopped,
) -> Reply<()> {
    let finished = stop.reason.as_str() == Some("finished");
    if !finished {
        let duration_ms = song(upstream, person, session)
            .await?
            .get("duration")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            * 1000;
        if stop.position_ms < SCROBBLE_AFTER_MS.min(duration_ms / 2) || duration_ms == 0 {
            return Ok(());
        }
    }
    let params = vec![
        ("id", session.to_owned()),
        ("submission", "true".to_owned()),
        ("time", now_ms().to_string()),
    ];
    upstream.call(person, "scrobble", &params).await.map(|_| ())
}

/// 队列存进 Navidrome 的 play queue，别的 Subsonic 客户端也能接着放；模式（随机、循环）上游不存，忽略。
pub(crate) async fn save_queue(
    upstream: &Upstream,
    person: &str,
    queue: &QueueSnapshot,
) -> Reply<()> {
    let mut params: Vec<(&'static str, String)> = queue
        .entries
        .iter()
        .map(|entry| ("id", entry.id.clone()))
        .collect();
    if let Some(current) = queue.current.and_then(|index| queue.entries.get(index)) {
        params.push(("current", current.id.clone()));
        params.push(("position", queue.position_ms.to_string()));
    }
    upstream
        .call(person, "savePlayQueue", &params)
        .await
        .map(|_| ())
}

pub(crate) async fn song(upstream: &Upstream, person: &str, id: &str) -> Reply<Value> {
    let reply = upstream
        .call(person, "getSong", &vec![("id", id.to_owned())])
        .await?;
    reply
        .get("song")
        .cloned()
        .ok_or_else(|| MediaFailure::protocol("Navidrome 的 getSong 回包里没有 song"))
}

fn accepts(profile: &DeviceProfile, types: &[&str], containers: &[&str]) -> bool {
    let any = |list: &[String], wanted: &[&str]| {
        list.iter().any(|have| {
            wanted
                .iter()
                .any(|w| !w.is_empty() && have.eq_ignore_ascii_case(w))
        })
    };
    any(&profile.direct_play, types) || any(&profile.container, containers)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

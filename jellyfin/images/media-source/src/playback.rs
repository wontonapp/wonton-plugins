//! `POST /playback` 取播放计划，`progress`/`stopped` 报进度。播放会话记在本人的 Jellyfin 令牌名下，
//! “继续观看”和“已看”由 Jellyfin 自己按人算。
//!
//! 适配器不存会话状态（规范“适配器进程”）：会话号就是 Jellyfin 的 `PlaySessionId` 加上报进度要的
//! 条目号、媒体源和播放方式，编码成一个不透明的串交给核心，核心原样带回。会话号里有这个人的名字，
//! 每次都和 `X-Wonton-User` 比对，别人拿到也用不了。

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::accounts::Account;
use crate::contract::{
    MediaTrack, PlaybackPlan, PlaybackRequest, Progress, ProviderStream, Stopped,
};
use crate::failure::{Code, Failure, Reply};
use crate::items;
use crate::jellyfin::{Jellyfin, encode, ms_to_ticks, ticks_to_ms};
use crate::profile::{bitrate, jellyfin_profile};

/// 计划的有效期。Jellyfin 的直链不过期，转码流闲置久了会被它收掉；过期后核心重取。
const PLAN_TTL_MS: u64 = 6 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// 这是谁的会话。
    pub u: String,
    /// Jellyfin 条目号。
    pub i: String,
    /// 媒体源（同一部片的不同版本）。
    pub m: String,
    /// Jellyfin 的 `PlaySessionId`。
    pub p: String,
    /// 走直链（`d`）还是 HLS 转码（`h`）。
    pub t: String,
}

impl Session {
    pub fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).expect("Session 总能序列化"))
    }

    /// 解开会话号并核对它属于这个人。
    pub fn decode(raw: &str, person: &str) -> Reply<Self> {
        let session: Session = URL_SAFE_NO_PAD
            .decode(raw)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| Failure::bad_request("会话号不是本提供方发的"))?;
        if session.u != person {
            return Err(Failure::new(Code::PermissionDenied, "这个会话不是这个人的"));
        }
        Ok(session)
    }

    fn hls(&self) -> bool {
        self.t == "h"
    }

    fn method(&self) -> &'static str {
        if self.hls() {
            "Transcode"
        } else {
            "DirectPlay"
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfPlaybackInfo {
    #[serde(default)]
    media_sources: Vec<JfSource>,
    play_session_id: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfSource {
    id: String,
    #[serde(default)]
    supports_direct_play: bool,
    transcoding_url: Option<String>,
    run_time_ticks: Option<u64>,
    #[serde(default)]
    media_streams: Vec<JfStream>,
    default_audio_stream_index: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfStream {
    #[serde(rename = "Type")]
    kind: String,
    index: i32,
    language: Option<String>,
    display_title: Option<String>,
}

pub async fn plan(
    jf: &Jellyfin,
    who: &Account,
    req: &PlaybackRequest,
    now_ms: u64,
) -> Reply<PlaybackPlan> {
    // 换音轨或开字幕时不走直链：直链里放的是文件原样，选哪条轨要播放器自己认 Jellyfin 的轨号，两端
    // 播放器的轨序和 Jellyfin 对不上。改走 HLS 让 Jellyfin 按轨号重封装（编码能直接拷贝时不重编）。
    let wants_tracks = req.subtitle.is_some() || req.audio.is_some();
    let body = json!({
        "UserId": who.user_id,
        "DeviceProfile": jellyfin_profile(&req.profile),
        "MaxStreamingBitrate": bitrate(&req.profile),
        "StartTimeTicks": ms_to_ticks(req.position_ms),
        "AudioStreamIndex": req.audio,
        "SubtitleStreamIndex": req.subtitle,
        "EnableDirectPlay": !wants_tracks,
        "EnableDirectStream": true,
        "EnableTranscoding": true,
        "AllowVideoStreamCopy": true,
        "AllowAudioStreamCopy": true,
        "AutoOpenLiveStream": false,
    });
    let path = format!(
        "/Items/{}/PlaybackInfo?userId={}",
        encode(&req.item.id),
        encode(&who.user_id)
    );
    let info: JfPlaybackInfo = jf.post(&path, &body, &who.identity(), "取播放信息").await?;
    if let Some(code) = info.error_code.as_deref() {
        return Err(playback_error(code));
    }
    let source = info
        .media_sources
        .into_iter()
        .next()
        .ok_or_else(|| Failure::new(Code::MediaGone, "这一条没有能放的文件"))?;
    let play_session = info
        .play_session_id
        .ok_or_else(|| Failure::new(Code::Protocol, "Jellyfin 没给播放会话号"))?;
    let direct = source.supports_direct_play && !wants_tracks;
    let session = Session {
        u: who.person.clone(),
        i: req.item.id.clone(),
        m: source.id.clone(),
        p: play_session,
        t: if direct { "d" } else { "h" }.into(),
    };
    let token = session.encode();
    let stream = if direct {
        ProviderStream::Direct(format!("/playback/{token}/stream"))
    } else {
        let url = source.transcoding_url.as_deref().ok_or_else(|| {
            Failure::new(
                Code::UnsupportedByDevice,
                "这台设备放不了这个文件，Jellyfin 也没给转码流",
            )
        })?;
        ProviderStream::Transcoded(format!("/playback/{token}/hls/{}", hls_tail(url)?))
    };
    report_start(jf, who, &session, req).await?;
    let entry = items::item(jf, who, &req.item.id).await?;
    let (audio, subtitles) = tracks(&source, req);
    Ok(PlaybackPlan {
        session: token,
        stream,
        origin_stream: None,
        segments: items::chapters(&entry),
        audio,
        subtitles,
        seekable: true,
        duration_ms: source.run_time_ticks.map(ticks_to_ms).unwrap_or(0),
        expires_at: now_ms + PLAN_TTL_MS,
    })
}

fn playback_error(code: &str) -> Failure {
    match code {
        "NotAllowed" => Failure::new(Code::PermissionDenied, "Jellyfin 不允许这个人播放这一条"),
        "NoCompatibleStream" => Failure::new(
            Code::UnsupportedByDevice,
            "这台设备放不了，Jellyfin 也转不出能放的流",
        ),
        "RateLimitExceeded" => {
            Failure::new(Code::RateLimited, "同时播放的人数到了 Jellyfin 的上限")
        }
        other => Failure::new(Code::UpstreamError, format!("Jellyfin 拒绝播放：{other}")),
    }
}

/// `TranscodingUrl` 形如 `/videos/<id>/master.m3u8?…`；取 `<id>/` 之后的部分，并去掉 URL 里的凭据，
/// 由 [`crate::proxy`] 转发时在请求头里补。
pub fn hls_tail(transcoding_url: &str) -> Reply<String> {
    let lower = transcoding_url.to_ascii_lowercase();
    let start = lower
        .find("/videos/")
        .ok_or_else(|| Failure::new(Code::Protocol, "转码地址不是 /videos/ 开头"))?;
    let rest = &transcoding_url[start + "/videos/".len()..];
    let (_, tail) = rest
        .split_once('/')
        .ok_or_else(|| Failure::new(Code::Protocol, "转码地址缺播放列表名"))?;
    Ok(strip_credentials(tail))
}

pub fn strip_credentials(path_and_query: &str) -> String {
    let Some((path, query)) = path_and_query.split_once('?') else {
        return path_and_query.to_owned();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("").to_ascii_lowercase();
            key != "apikey" && key != "api_key"
        })
        .collect();
    if kept.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{}", kept.join("&"))
    }
}

fn tracks(source: &JfSource, req: &PlaybackRequest) -> (Vec<MediaTrack>, Vec<MediaTrack>) {
    let default_audio = req
        .audio
        .map(i32::from)
        .or(source.default_audio_stream_index);
    let pick = |kind: &str, label: &'static str, chosen: Option<i32>| -> Vec<MediaTrack> {
        source
            .media_streams
            .iter()
            .filter(|s| s.kind == kind)
            .filter_map(|s| {
                Some(MediaTrack {
                    index: u16::try_from(s.index).ok()?,
                    language: s.language.clone().unwrap_or_else(|| "und".into()),
                    label: s.display_title.clone(),
                    kind: label,
                    selected: chosen == Some(s.index),
                })
            })
            .collect()
    };
    (
        pick("Audio", "audio", default_audio),
        pick("Subtitle", "subtitle", req.subtitle.map(i32::from)),
    )
}

fn report_body(session: &Session, position_ms: u64) -> Value {
    json!({
        "ItemId": session.i,
        "MediaSourceId": session.m,
        "PlaySessionId": session.p,
        "PositionTicks": ms_to_ticks(position_ms),
        "PlayMethod": session.method(),
        "CanSeek": true,
    })
}

async fn report_start(
    jf: &Jellyfin,
    who: &Account,
    session: &Session,
    req: &PlaybackRequest,
) -> Reply<()> {
    let mut body = report_body(session, req.position_ms);
    body["AudioStreamIndex"] = json!(req.audio);
    body["SubtitleStreamIndex"] = json!(req.subtitle);
    jf.call(
        Method::POST,
        "/Sessions/Playing",
        Some(&body),
        &who.identity(),
        "报开始播放",
    )
    .await
}

pub async fn progress(
    jf: &Jellyfin,
    who: &Account,
    session: &Session,
    report: &Progress,
) -> Reply<()> {
    let mut body = report_body(session, report.position_ms);
    body["IsPaused"] = json!(report.paused);
    body["EventName"] = json!("TimeUpdate");
    jf.call(
        Method::POST,
        "/Sessions/Playing/Progress",
        Some(&body),
        &who.identity(),
        "报进度",
    )
    .await
}

/// 报结束，再收掉这个会话的转码进程；转码进程已经没了不算失败。
pub async fn stopped(
    jf: &Jellyfin,
    who: &Account,
    session: &Session,
    report: &Stopped,
) -> Reply<()> {
    let body = report_body(session, report.position_ms);
    jf.call(
        Method::POST,
        "/Sessions/Playing/Stopped",
        Some(&body),
        &who.identity(),
        "报结束",
    )
    .await?;
    if session.hls() {
        let path = format!(
            "/Videos/ActiveEncodings?deviceId={}&playSessionId={}",
            encode(&who.device),
            encode(&session.p)
        );
        match jf
            .call(Method::DELETE, &path, None, &who.identity(), "收转码")
            .await
        {
            Err(f) if f.code != Code::MediaGone => return Err(f),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            u: "mom".into(),
            i: "item".into(),
            m: "src".into(),
            p: "ps".into(),
            t: "h".into(),
        }
    }

    #[test]
    fn session_round_trips_and_binds_to_its_person() {
        let raw = session().encode();
        assert_eq!(Session::decode(&raw, "mom").unwrap(), session());
        assert_eq!(
            Session::decode(&raw, "dad").unwrap_err().code,
            Code::PermissionDenied
        );
        assert_eq!(
            Session::decode("garbage!", "mom").unwrap_err().code,
            Code::Protocol
        );
    }

    #[test]
    fn hls_tail_keeps_the_playlist_and_drops_the_key() {
        let url =
            "/videos/0b744685-37ca/master.m3u8?DeviceId=d&ApiKey=secret&VideoCodec=h264&api_key=x";
        assert_eq!(
            hls_tail(url).unwrap(),
            "master.m3u8?DeviceId=d&VideoCodec=h264"
        );
        assert!(hls_tail("/audio/x/master.m3u8").is_err());
    }

    #[test]
    fn playback_errors_map_to_contract_codes() {
        assert_eq!(playback_error("NotAllowed").code, Code::PermissionDenied);
        assert_eq!(
            playback_error("NoCompatibleStream").code,
            Code::UnsupportedByDevice
        );
        assert_eq!(playback_error("RateLimitExceeded").code, Code::RateLimited);
    }
}

//! 契约的设备能力档翻成 Jellyfin 的 `DeviceProfile`。Jellyfin 按它判断能不能直接放、要不要转码，所以这里
//! 只说手机能解什么，不替 Jellyfin 做判断。
//!
//! 转码目标固定为 HLS（TS 封装、H.264 + AAC）：iOS 的 AVPlayer 只认 HLS 这一种自适应流，Android 的 Media3
//! 也认，两端共用一条转码路径。字幕要么交给 HLS 做文字轨（文本字幕），要么由 Jellyfin 烧进画面（图形字幕）。

use serde_json::{Value, json};

use crate::contract::DeviceProfile;

/// 壳没给码率上限时按 120 Mbps 算：比 4K 蓝光原盘的常见码率高，等于不限。
const UNLIMITED_BITRATE: u32 = 120_000_000;

const VIDEO_CODECS: &[(&str, &str)] = &[
    ("avc1", "h264"),
    ("avc3", "h264"),
    ("h264", "h264"),
    ("hvc1", "hevc"),
    ("hev1", "hevc"),
    ("hevc", "hevc"),
    ("h265", "hevc"),
    ("av01", "av1"),
    ("av1", "av1"),
    ("vp09", "vp9"),
    ("vp9", "vp9"),
    ("vp8", "vp8"),
];

const AUDIO_CODECS: &[(&str, &str)] = &[
    ("mp4a", "aac"),
    ("aac", "aac"),
    ("ac-3", "ac3"),
    ("ac3", "ac3"),
    ("ec-3", "eac3"),
    ("eac3", "eac3"),
    ("opus", "opus"),
    ("flac", "flac"),
    ("alac", "alac"),
    ("mp3", "mp3"),
    ("mp4a.40.34", "mp3"),
];

/// `video/mp4; codecs="avc1.640028,mp4a.40.2"` 里的编解码按 Jellyfin 的名字收拢。
fn codecs(profile: &DeviceProfile) -> (Vec<&'static str>, Vec<&'static str>) {
    let mut video = Vec::new();
    let mut audio = Vec::new();
    for entry in &profile.direct_play {
        let tokens: Vec<String> = match entry.split_once("codecs=") {
            Some((_, list)) => list
                .trim_matches(|c| c == '"' || c == '\'' || c == ' ')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_ascii_lowercase())
                .collect(),
            None if !entry.contains('/') => vec![entry.trim().to_ascii_lowercase()],
            None => Vec::new(),
        };
        for token in tokens {
            push_match(&token, VIDEO_CODECS, &mut video);
            push_match(&token, AUDIO_CODECS, &mut audio);
        }
    }
    (video, audio)
}

fn push_match(token: &str, table: &[(&str, &'static str)], out: &mut Vec<&'static str>) {
    // 先比完整写法（`mp4a.40.34` 是 MP3），再比点号前的四字码。
    let hit = table.iter().find(|(raw, _)| *raw == token).or_else(|| {
        let head = token.split('.').next().unwrap_or(token);
        table.iter().find(|(raw, _)| *raw == head)
    });
    if let Some((_, name)) = hit
        && !out.contains(name)
    {
        out.push(name);
    }
}

/// Jellyfin 的码率字段是 32 位有符号整数，超过 `i32::MAX` 整个请求回 400（12.4 实测）；契约是 `u32`，
/// 客户端把“不限”报成 `u32::MAX`，这里收进 Jellyfin 能收的范围。
pub fn bitrate(profile: &DeviceProfile) -> u32 {
    if profile.max_bitrate == 0 {
        UNLIMITED_BITRATE
    } else {
        profile.max_bitrate.min(i32::MAX as u32)
    }
}

/// 翻成 Jellyfin `DeviceProfile`。手机一个能直接解的都没报时，直放清单为空，全部走转码。
pub fn jellyfin_profile(profile: &DeviceProfile) -> Value {
    let (video, audio) = codecs(profile);
    let containers: Vec<String> = profile
        .container
        .iter()
        .map(|c| c.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    let direct = if video.is_empty() || containers.is_empty() {
        Vec::new()
    } else {
        vec![json!({
            "Type": "Video",
            "Container": containers.join(","),
            "VideoCodec": video.join(","),
            "AudioCodec": audio.join(","),
        })]
    };
    let mut codec_profiles = Vec::new();
    if !profile.hdr {
        // 不报 HDR 的设备只直放 SDR，HDR 片交给 Jellyfin 转成 SDR。
        codec_profiles.push(json!({
            "Type": "Video",
            "Conditions": [{"Condition": "EqualsAny", "Property": "VideoRangeType", "Value": "SDR", "IsRequired": false}],
        }));
    }
    json!({
        "Name": "Wonton",
        "MaxStreamingBitrate": bitrate(profile),
        "MaxStaticBitrate": bitrate(profile),
        "DirectPlayProfiles": direct,
        "TranscodingProfiles": [{
            "Type": "Video", "Container": "ts", "Protocol": "hls", "Context": "Streaming",
            "VideoCodec": "h264", "AudioCodec": "aac", "MaxAudioChannels": "6",
            "BreakOnNonKeyFrames": true, "MinSegments": 1,
        }],
        "CodecProfiles": codec_profiles,
        "SubtitleProfiles": [
            {"Format": "vtt", "Method": "Hls"},
            {"Format": "srt", "Method": "Hls"},
            {"Format": "ass", "Method": "Hls"},
            {"Format": "ssa", "Method": "Hls"},
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(direct: &[&str], container: &[&str]) -> DeviceProfile {
        DeviceProfile {
            direct_play: direct.iter().map(|s| s.to_string()).collect(),
            max_bitrate: 0,
            hdr: false,
            container: container.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn mime_codecs_become_jellyfin_codec_names() {
        let p = profile(
            &[r#"video/mp4; codecs="avc1.640028,hvc1.2.4.L153,mp4a.40.2,ec-3""#],
            &["mp4", ".MOV"],
        );
        let jf = jellyfin_profile(&p);
        let direct = &jf["DirectPlayProfiles"][0];
        assert_eq!(direct["Container"], "mp4,mov");
        assert_eq!(direct["VideoCodec"], "h264,hevc");
        assert_eq!(direct["AudioCodec"], "aac,eac3");
    }

    #[test]
    fn bare_codec_names_are_accepted() {
        let jf = jellyfin_profile(&profile(&["h264", "aac"], &["mp4"]));
        assert_eq!(jf["DirectPlayProfiles"][0]["VideoCodec"], "h264");
        assert_eq!(jf["DirectPlayProfiles"][0]["AudioCodec"], "aac");
    }

    #[test]
    fn nothing_decodable_means_transcode_everything() {
        let jf = jellyfin_profile(&profile(&[], &["mp4"]));
        assert_eq!(jf["DirectPlayProfiles"], json!([]));
        assert_eq!(jf["TranscodingProfiles"][0]["Protocol"], "hls");
    }

    #[test]
    fn zero_bitrate_means_unlimited_and_sdr_only_without_hdr() {
        let jf = jellyfin_profile(&profile(&["h264"], &["mp4"]));
        assert_eq!(jf["MaxStreamingBitrate"], UNLIMITED_BITRATE);
        assert_eq!(jf["CodecProfiles"][0]["Conditions"][0]["Value"], "SDR");
        let mut hdr = profile(&["h264"], &["mp4"]);
        hdr.hdr = true;
        hdr.max_bitrate = 8_000_000;
        let jf = jellyfin_profile(&hdr);
        assert_eq!(jf["MaxStreamingBitrate"], 8_000_000);
        assert_eq!(jf["CodecProfiles"], json!([]));
    }

    #[test]
    fn bitrate_fits_in_jellyfin_int32() {
        let mut p = profile(&["h264"], &["mp4"]);
        p.max_bitrate = u32::MAX;
        let jf = jellyfin_profile(&p);
        assert_eq!(jf["MaxStreamingBitrate"], i32::MAX);
        assert_eq!(jf["MaxStaticBitrate"], i32::MAX);
    }
}

//! 共享文件夹里的只读路径访问。每一段都用 `openat` + `O_NOFOLLOW` 打开，路径上有软链接就拒绝；
//! 挂载点本身是内核交给容器的，只有共享文件夹的名字参与引用，容器里的挂载点不露出去。
//!
//! 遍历按深度优先，同一时刻每条深度各留一个目录 fd；文件名按字节保存，非 UTF-8 的也照样返回。

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags, Stat, fstat, openat, statat};
use rustix::io::Errno;

use crate::report::WalkReport;

/// `transfers` 放设备经核心上传的文件。
pub const SHARES: [&str; 6] = [
    "documents",
    "photos",
    "music",
    "videos",
    "downloads",
    "transfers",
];

pub const MAX_DEPTH: usize = 64;

#[derive(Debug)]
pub enum FsError {
    Invalid(String),
    InvalidParam(String),
    TimezoneUnavailable(String),
    NotFound(String),
    Denied(String),
    TooLarge(u64),
    NotText,
    NotUtf8,
    Io(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(msg) => write!(f, "路径不合法：{msg}"),
            Self::InvalidParam(msg) => write!(f, "参数不合法：{msg}"),
            Self::TimezoneUnavailable(msg) => write!(f, "日期过滤的时区不可用：{msg}"),
            Self::NotFound(path) => write!(f, "不存在：{path}"),
            Self::Denied(path) => write!(f, "没有权限：{path}"),
            Self::TooLarge(size) => write!(f, "文件太大：{size} 字节"),
            Self::NotText => f.write_str("不是文字文件"),
            Self::NotUtf8 => f.write_str("不是 UTF-8 编码的文字文件"),
            Self::Io(msg) => write!(f, "读取失败：{msg}"),
        }
    }
}

impl From<io::Error> for FsError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

fn map_errno(errno: Errno, path: &str) -> FsError {
    match errno {
        Errno::NOENT | Errno::NOTDIR => FsError::NotFound(path.to_owned()),
        Errno::ACCESS | Errno::PERM => FsError::Denied(path.to_owned()),
        Errno::LOOP => FsError::Invalid(format!("{path} 是软链接")),
        Errno::NAMETOOLONG => FsError::Invalid(format!("{path} 太长")),
        other => FsError::Io(format!("{path}: {other}")),
    }
}

pub fn display(share: &str, rel: &[OsString]) -> String {
    let mut out = share.to_owned();
    for segment in rel {
        out.push('/');
        out.push_str(&segment.to_string_lossy());
    }
    out
}

fn is_utf8(name: &OsStr) -> bool {
    name.to_str().is_some()
}

pub struct Budget {
    pub max_entries: u64,
    pub entries: u64,
}

impl Budget {
    pub fn new(max_entries: u64) -> Self {
        Self {
            max_entries,
            entries: 0,
        }
    }

    pub fn tick(&mut self) -> bool {
        if self.entries >= self.max_entries {
            true
        } else {
            self.entries += 1;
            false
        }
    }
}

#[derive(Debug)]
pub struct Found {
    pub share: &'static str,
    pub rel: Vec<OsString>,
    pub stat: Stat,
}

pub struct FilesFs {
    root: PathBuf,
}

impl FilesFs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn share(&self, share: &str) -> Result<&'static str, FsError> {
        SHARES
            .iter()
            .copied()
            .find(|known| *known == share)
            .ok_or_else(|| {
                FsError::InvalidParam(format!("共享文件夹 {share}，可选：{}", SHARES.join("、")))
            })
    }

    /// 相对路径按 URI 路径段检查：不能是绝对路径，不能有空段、`.`、`..`。
    pub fn split_rel(&self, path: &str) -> Result<Vec<OsString>, FsError> {
        if path.starts_with('/') {
            return Err(FsError::Invalid(format!("{path} 是绝对路径")));
        }
        if path.is_empty() {
            return Ok(Vec::new());
        }
        let segments: Vec<&str> = path.split('/').collect();
        for (index, segment) in segments.iter().enumerate() {
            if segment.is_empty() {
                return Err(FsError::Invalid(format!(
                    "{path} 第 {} 段是空的",
                    index + 1
                )));
            }
            if *segment == "." || *segment == ".." {
                return Err(FsError::Invalid(format!("{path} 含 {segment}")));
            }
            if segment.contains('\0') {
                return Err(FsError::Invalid(format!("{path} 含 NUL")));
            }
        }
        Ok(segments.into_iter().map(OsString::from).collect())
    }

    fn open_root(&self, share: &str) -> Result<std::os::fd::OwnedFd, FsError> {
        let path = self.root.join(share);
        openat(
            rustix::fs::CWD,
            path.as_path(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|errno| map_errno(errno, &display(share, &[])))
    }

    /// 打开共享文件夹里的目录。`rel` 为空时是共享文件夹的根。
    pub fn open_dir(&self, share: &str, rel: &[OsString]) -> Result<std::os::fd::OwnedFd, FsError> {
        let share = self.share(share)?;
        let mut dir = self.open_root(share)?;
        for (index, segment) in rel.iter().enumerate() {
            let here = display(share, &rel[..=index]);
            dir = openat(
                &dir,
                segment.as_os_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|errno| map_errno(errno, &here))?;
        }
        Ok(dir)
    }

    /// 打开普通文件。最后一段和中间每一段都不跟随软链接。
    pub fn open_file(&self, share: &str, rel: &[OsString]) -> Result<(File, Stat), FsError> {
        let share = self.share(share)?;
        let here = display(share, rel);
        let (last, dirs) = rel
            .split_last()
            .ok_or_else(|| FsError::Invalid(format!("{here} 是目录，不是文件")))?;
        let dir = self.open_dir(share, dirs)?;
        let fd = openat(
            &dir,
            last.as_os_str(),
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|errno| map_errno(errno, &here))?;
        let stat = fstat(&fd).map_err(|errno| map_errno(errno, &here))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(match FileType::from_raw_mode(stat.st_mode) {
                FileType::Directory => FsError::Invalid(format!("{here} 是目录")),
                FileType::Symlink => FsError::Invalid(format!("{here} 是软链接")),
                _ => FsError::Invalid(format!("{here} 不是普通文件")),
            });
        }
        Ok((File::from(fd), stat))
    }

    /// 递归遍历目录。访问函数返回 false 时停下。预算按读到的条目计。
    pub fn walk(
        &self,
        share: &str,
        base: &[OsString],
        budget: &mut Budget,
        mut visit: impl FnMut(&Found) -> bool,
    ) -> Result<WalkReport, FsError> {
        let share = self.share(share)?;
        let root = self.open_dir(share, base)?;
        let mut report = WalkReport::default();
        let stopped = self.walk_dir(
            share,
            &root,
            base,
            base.len(),
            budget,
            &mut report,
            &mut visit,
        )?;
        report.stopped = stopped;
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_dir(
        &self,
        share: &'static str,
        dir: &std::os::fd::OwnedFd,
        prefix: &[OsString],
        depth: usize,
        budget: &mut Budget,
        report: &mut WalkReport,
        visit: &mut impl FnMut(&Found) -> bool,
    ) -> Result<bool, FsError> {
        let entries = match Dir::read_from(dir) {
            Ok(entries) => entries,
            Err(errno) if errno == Errno::ACCESS || errno == Errno::PERM => {
                report.denied.add(display(share, prefix));
                return Ok(false);
            }
            Err(errno) => return Err(map_errno(errno, &display(share, prefix))),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    report.stat_failed.add(display(share, prefix));
                    continue;
                }
            };
            // 预算在读取条目时扣，超大目录不会先整个读进内存。
            if budget.tick() {
                report.stopped = true;
                return Ok(true);
            }
            let name = OsStr::from_bytes(entry.file_name().to_bytes()).to_owned();
            if name == "." || name == ".." {
                continue;
            }
            let mut rel = prefix.to_vec();
            rel.push(name.clone());
            let here = display(share, &rel);
            if !is_utf8(&name) {
                report.non_utf8.add(here.clone());
            }
            let stat = match statat(dir, name.as_os_str(), AtFlags::SYMLINK_NOFOLLOW) {
                Ok(stat) => stat,
                Err(Errno::NOENT) => continue,
                Err(errno) => {
                    if errno == Errno::ACCESS || errno == Errno::PERM {
                        report.denied.add(here);
                    } else {
                        report.stat_failed.add(here);
                    }
                    continue;
                }
            };
            match FileType::from_raw_mode(stat.st_mode) {
                FileType::RegularFile => {
                    let found = Found { share, rel, stat };
                    if !visit(&found) {
                        report.stopped = true;
                        return Ok(true);
                    }
                }
                FileType::Directory => {
                    if depth + 1 > MAX_DEPTH {
                        report.too_deep.add(here);
                        continue;
                    }
                    let flags =
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
                    match openat(dir, name.as_os_str(), flags, Mode::empty()) {
                        Ok(child) => {
                            let stop = self.walk_dir(
                                share,
                                &child,
                                &rel,
                                depth + 1,
                                budget,
                                report,
                                visit,
                            )?;
                            if stop {
                                return Ok(true);
                            }
                        }
                        Err(Errno::ACCESS | Errno::PERM) => report.denied.add(here),
                        Err(Errno::NOENT) => {}
                        Err(errno) => return Err(map_errno(errno, &here)),
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }

    /// 读字节范围，附带最多 3 个前置字节以验证 UTF-8 中间偏移；返回内容、总大小和起点。
    pub fn read_range(
        &self,
        share: &str,
        rel: &[OsString],
        offset: u64,
        limit: u64,
    ) -> Result<(Vec<u8>, u64, u64), FsError> {
        use std::io::{Read, Seek, SeekFrom};

        let (mut file, stat) = self.open_file(share, rel)?;
        let size = stat.st_size.max(0) as u64;
        if offset > size {
            return Err(FsError::InvalidParam(format!(
                "offset {offset} 超过文件大小 {size}"
            )));
        }
        let start = offset.saturating_sub(3);
        let read_limit = offset - start + limit + 4;
        file.seek(SeekFrom::Start(start))?;
        let mut buf = Vec::with_capacity(read_limit as usize);
        file.take(read_limit).read_to_end(&mut buf)?;
        Ok((buf, size, start))
    }
}

/// `wonton-share:` 引用的后半段：第一段是共享文件夹，其余按 URI 路径规则逐字节百分号编码。
pub fn link_uri(share: &str, rel: &[OsString]) -> String {
    const SET: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'%')
        .add(b'<')
        .add(b'>')
        .add(b'?')
        .add(b'[')
        .add(b']')
        .add(b'^')
        .add(b'`')
        .add(b'{')
        .add(b'}');
    let mut uri = String::with_capacity(16 + share.len());
    uri.push_str("wonton-share:");
    uri.push_str(share);
    for segment in rel {
        uri.push('/');
        uri.push_str(&percent_encoding::percent_encode(segment.as_bytes(), SET).to_string());
    }
    uri
}

/// 引用里的显示名：最后一段，非 UTF-8 字节用具替换字符的显示形式。
pub fn file_name(rel: &[OsString]) -> String {
    rel.last()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 文件类别的筛选值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Audio,
    Video,
    Document,
    Text,
    Other,
}

pub fn kind_of(name: &str) -> Kind {
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    match mime.type_().as_str() {
        "image" => Kind::Image,
        "audio" => Kind::Audio,
        "video" => Kind::Video,
        "text" => Kind::Text,
        _ => match mime.essence_str() {
            "application/pdf"
            | "application/msword"
            | "application/rtf"
            | "application/epub+zip"
            | "application/vnd.oasis.opendocument.text"
            | "application/vnd.oasis.opendocument.spreadsheet"
            | "application/vnd.oasis.opendocument.presentation" => Kind::Document,
            e if e.starts_with("application/vnd.openxmlformats-officedocument.") => Kind::Document,
            "application/json"
            | "application/xml"
            | "application/yaml"
            | "application/toml"
            | "application/javascript"
            | "application/x-sh" => Kind::Text,
            _ => Kind::Other,
        },
    }
}

pub fn mime_of(name: &str) -> String {
    mime_guess::from_path(name)
        .first_or_octet_stream()
        .essence_str()
        .to_owned()
}

/// 文字判定：含 NUL 是二进制；不含 NUL 但不是 UTF-8 单独一类。
pub enum TextKind {
    Text,
    Binary,
    NotUtf8,
}

pub fn classify_text(bytes: &[u8]) -> TextKind {
    if bytes.contains(&0) {
        TextKind::Binary
    } else if std::str::from_utf8(bytes).is_err() {
        TextKind::NotUtf8
    } else {
        TextKind::Text
    }
}

fn zone_from_env() -> Result<Option<jiff::tz::TimeZone>, String> {
    match std::env::var("TZ") {
        Err(_) => Ok(None),
        Ok(name) if name.is_empty() => Ok(None),
        Ok(name) => zone_from_name(&name),
    }
}

/// TZ 先按 IANA 名字认；认不出再按 POSIX 写法，`:/路径` 读 TZif 文件。
fn zone_from_name(name: &str) -> Result<Option<jiff::tz::TimeZone>, String> {
    if let Ok(zone) = jiff::tz::TimeZone::get(name) {
        return Ok(Some(zone));
    }
    let parsed = if let Some(path) = name.strip_prefix(':') {
        let bytes =
            std::fs::read(path).map_err(|err| format!("TZ={name} 的时区文件读不了：{err}"))?;
        jiff::tz::TimeZone::tzif(path, &bytes)
    } else {
        jiff::tz::TimeZone::posix(name)
    };
    parsed
        .map(Some)
        .map_err(|err| format!("TZ={name} 不是有效时区：{err}"))
}

/// RFC 3339（带 `T`）或 `YYYY-MM-DD` 转 Unix 秒。只有日期时按 `TZ` 解释，`TZ` 没设才用系统时区。
pub fn parse_time(text: &str) -> Result<i64, FsError> {
    if text.contains('T') {
        return text
            .parse::<jiff::Timestamp>()
            .map(|time| time.as_second())
            .map_err(|err| FsError::InvalidParam(format!("{text} 不是 RFC 3339 时间：{err}")));
    }
    let date = jiff::civil::Date::strptime("%Y-%m-%d", text)
        .map_err(|err| FsError::InvalidParam(format!("{text} 不是日期：{err}")))?;
    let zone = zone_from_env()
        .map_err(FsError::TimezoneUnavailable)?
        .unwrap_or_else(jiff::tz::TimeZone::system);
    date.to_zoned(zone)
        .map(|zoned| zoned.timestamp().as_second())
        .map_err(|err| FsError::InvalidParam(format!("{text} 换不成时间：{err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    #[test]
    fn uri_encodes_segments() {
        let uri = link_uri("documents", &os(&["中文 目录", "a#b%c.txt"]));
        assert_eq!(
            uri,
            "wonton-share:documents/%E4%B8%AD%E6%96%87%20%E7%9B%AE%E5%BD%95/a%23b%25c.txt"
        );
    }

    #[test]
    fn uri_encodes_non_utf8_bytes() {
        let name = OsStr::from_bytes(b"bad\xff.txt").to_owned();
        assert_eq!(
            link_uri("documents", &[name]),
            "wonton-share:documents/bad%FF.txt"
        );
    }

    static TZ_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_tz<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
        let _guard = TZ_LOCK.lock().unwrap();
        let old = std::env::var("TZ").ok();
        unsafe {
            match value {
                Some(value) => std::env::set_var("TZ", value),
                None => std::env::remove_var("TZ"),
            }
        }
        let out = f();
        unsafe {
            match old {
                Some(old) => std::env::set_var("TZ", old),
                None => std::env::remove_var("TZ"),
            }
        }
        out
    }

    #[test]
    fn time_parsing() {
        with_tz(Some("UTC"), || {
            assert_eq!(parse_time("2026-11-03T01:02:03Z").unwrap(), 1_793_667_723);
            assert_eq!(
                parse_time("2026-11-03T01:02:03+08:00").unwrap(),
                1_793_638_923
            );
            assert!(parse_time("2026-02-31").is_err());
            assert_eq!(parse_time("1970-01-01").unwrap(), 0);
        });
    }

    #[test]
    fn invalid_timezone_is_an_error() {
        for value in ["Not/AZone", "Mars/Olympus"] {
            with_tz(Some(value), || {
                assert!(parse_time("2026-01-01").is_err(), "TZ={value}");
            });
        }
    }

    #[test]
    fn posix_timezone_is_accepted() {
        with_tz(Some("CST-8"), || {
            assert_eq!(parse_time("1970-01-01").unwrap(), -28_800);
        });
    }

    #[test]
    fn missing_timezone_file_only_blocks_date_only_filters() {
        let dir = tempfile::tempdir().unwrap();
        let tz = format!(":{}", dir.path().join("missing-tzif").display());
        with_tz(Some(&tz), || {
            assert!(matches!(
                parse_time("1970-01-01"),
                Err(FsError::TimezoneUnavailable(_))
            ));
            assert_eq!(parse_time("1970-01-01T00:00:00Z").unwrap(), 0);
        });
    }

    #[test]
    fn date_only_uses_the_given_timezone() {
        with_tz(Some("Asia/Shanghai"), || {
            assert_eq!(parse_time("1970-01-01").unwrap(), -28_800);
        });
    }
}

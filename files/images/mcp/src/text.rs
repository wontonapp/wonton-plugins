//! UTF-8 分段读取：只有跨过请求偏移的完整合法字符才允许对齐。

use crate::fs::FsError;

pub(crate) fn read_chunk(
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Result<(usize, &str), FsError> {
    if offset > bytes.len() {
        return Err(FsError::Io("读取期间文件缩短，请重新读取".to_owned()));
    }
    let start = aligned_start(bytes, offset)?;
    let bytes = &bytes[start..];
    let mut end = bytes.len().min(limit);
    if bytes[..end].contains(&0) {
        return Err(FsError::NotText);
    }
    match std::str::from_utf8(&bytes[..end]) {
        Ok(text) => return Ok((start - offset, text)),
        Err(err) if err.error_len().is_some() => return Err(FsError::NotUtf8),
        Err(err) if err.valid_up_to() > 0 => end = err.valid_up_to(),
        Err(_) => {
            end = character_len(bytes[0]).ok_or(FsError::NotUtf8)?;
            if end > bytes.len() {
                return Err(FsError::NotUtf8);
            }
        }
    }
    let text = std::str::from_utf8(&bytes[..end]).map_err(|_| FsError::NotUtf8)?;
    Ok((start - offset, text))
}

fn aligned_start(bytes: &[u8], offset: usize) -> Result<usize, FsError> {
    if offset == bytes.len() || bytes[offset] & 0xC0 != 0x80 {
        return Ok(offset);
    }
    let mut start = offset;
    while start > 0 && bytes[start] & 0xC0 == 0x80 {
        start -= 1;
    }
    let width = character_len(bytes[start]).ok_or(FsError::NotUtf8)?;
    let end = start + width;
    if !(start < offset && offset < end && end <= bytes.len()) {
        return Err(FsError::NotUtf8);
    }
    std::str::from_utf8(&bytes[start..end]).map_err(|_| FsError::NotUtf8)?;
    Ok(end)
}

fn character_len(first: u8) -> Option<usize> {
    match first {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

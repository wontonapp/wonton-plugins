//! Navidrome 的 `media_source` 适配器：核心按插件规范“媒体源：路径”调进来，这里翻成 `OpenSubsonic` 调 Navidrome。
//!
//! 只做字段映射、错误码归一和身份注入，不存任务、会话和队列（规范“媒体源：适配器进程”）。和 Navidrome 共用网络
//! 命名空间，经 `127.0.0.1` 带 `Remote-User` 讲 `/rest/*`：Navidrome 的代理白名单里有 `127.0.0.1/32`，
//! 上游不需要每个人的口令。

mod bytes;
pub mod failure;
mod items;
pub mod keys;
mod playback;
pub mod routes;
pub mod upstream;
pub mod wire;

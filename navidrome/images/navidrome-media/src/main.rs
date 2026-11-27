//! 子命令：
//! - `serve`（默认）：监听 `NAVIDROME_MEDIA_LISTEN`（默认 `0.0.0.0:8090`），上游是 `NAVIDROME_URL`
//!   （默认 `http://127.0.0.1:4533`，同一个网络命名空间）；
//! - `healthcheck`：打一次本机的能力探测，给 compose 的健康检查用（镜像里没有 wget）；
//! - `credentials issue <名字>`、`credentials revoke <名字>`：核心的凭据命令，值从标准输入读。
//!
//! 凭据存在 `NAVIDROME_MEDIA_KEYS`（默认 `/data/keys`）。

use std::process::ExitCode;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use wonton_navidrome_media::keys::KeyStore;
use wonton_navidrome_media::routes::{AppState, router};
use wonton_navidrome_media::upstream::Upstream;

fn setting(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let keys = KeyStore::new(setting("NAVIDROME_MEDIA_KEYS", "/data/keys"));
    let listen = setting("NAVIDROME_MEDIA_LISTEN", "0.0.0.0:8090");
    let outcome = match args.as_slice() {
        [] | ["serve"] => serve(&listen, keys).await,
        ["healthcheck"] => healthcheck(&listen).await,
        ["credentials", "issue", name] => {
            let mut value = String::new();
            match tokio::io::stdin().read_to_string(&mut value).await {
                Ok(_) => keys.issue(name, &value).map_err(|e| e.to_string()),
                Err(e) => Err(format!("standard input unreadable: {e}")),
            }
        }
        ["credentials", "revoke", name] => keys.revoke(name).map_err(|e| e.to_string()),
        _ => Err("usage: serve | healthcheck | credentials issue|revoke <name>".to_owned()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(listen: &str, keys: KeyStore) -> Result<(), String> {
    let state = AppState {
        upstream: Upstream::new(&setting("NAVIDROME_URL", "http://127.0.0.1:4533")),
        keys: Arc::new(keys),
    };
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|e| format!("cannot listen on {listen}: {e}"))?;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(stop_signal())
        .await
        .map_err(|e| e.to_string())
}

/// 容器里它是 1 号进程，不处理 SIGTERM 的话 `docker stop` 要等满超时再强杀。
async fn stop_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
        Ok(mut terminate) => {
            tokio::select! {
                _ = terminate.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
        }
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

async fn healthcheck(listen: &str) -> Result<(), String> {
    let port = listen.rsplit(':').next().unwrap_or("8090");
    let url = format!("http://127.0.0.1:{port}/_wonton/capabilities");
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("{url} answered {}", response.status()))
    }
}

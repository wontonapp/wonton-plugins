//! 入口：默认起服务；`healthcheck` 给镜像的健康检查，`credential issue|revoke <名字>` 是清单里
//! `provides.media_source.credentials` 的命令（值从标准输入读一行）。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use wonton_immich_media::accounts::{Accounts, Owner};
use wonton_immich_media::{App, DEFAULT_PORT, auth, immich, router, sweep_stale_uploads};

fn env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("缺环境变量 {name}"))
}

fn state_dir() -> PathBuf {
    std::env::var_os("STATE_DIR")
        .map(Into::into)
        .unwrap_or_else(|| "/state".into())
}

fn port() -> Result<u16, String> {
    match std::env::var("PORT") {
        Ok(value) => value.parse().map_err(|_| format!("PORT={value} 不是端口")),
        Err(_) => Ok(DEFAULT_PORT),
    }
}

fn credential(args: &[String]) -> ExitCode {
    let state = state_dir();
    let result = match (args.first().map(String::as_str), args.get(1)) {
        (Some("issue"), Some(name)) => {
            let mut value = String::new();
            std::io::stdin()
                .read_line(&mut value)
                .and_then(|_| auth::issue(&state, name, &value))
        }
        (Some("revoke"), Some(name)) => auth::revoke(&state, name),
        _ => {
            eprintln!("用法：credential issue|revoke <名字>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("凭据命令失败：{err}");
            ExitCode::FAILURE
        }
    }
}

fn healthcheck(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let request = b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let mut buf = Vec::new();
    if stream.write_all(request).is_err() || stream.read_to_end(&mut buf).is_err() {
        return false;
    }
    buf.starts_with(b"HTTP/1.1 200") || buf.starts_with(b"HTTP/1.0 200")
}

async fn serve(port: u16) -> Result<(), String> {
    let owner = Owner {
        name: env("WONTON_OWNER_NAME")?,
        email: env("WONTON_OWNER_EMAIL")?.to_ascii_lowercase(),
        password: env("IMMICH_ADMIN_PASSWORD")?,
    };
    let state = state_dir();
    let client = immich::Client::new(env("IMMICH_URL")?);
    let app = App {
        accounts: Arc::new(Accounts::new(&state, owner, client.clone())),
        immich: client,
        state,
        household: std::env::var("HOUSEHOLD_USER")
            .ok()
            .filter(|v| !v.is_empty()),
    };
    sweep_stale_uploads(&app, Duration::from_secs(7 * 24 * 3600));
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|err| format!("监听 {port} 失败：{err}"))?;
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|err| format!("服务出错：{err}"))
}

/// 容器里是 1 号进程，没装处理函数的 SIGTERM 会被忽略，`docker stop` 只能等到超时再杀。
async fn shutdown() {
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term => {}
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("credential") => return credential(&args[1..]),
        Some("healthcheck") => {
            let ok = port().is_ok_and(healthcheck);
            return if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        _ => {}
    }
    let result = match port() {
        Ok(port) => serve(port).await,
        Err(err) => Err(err),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

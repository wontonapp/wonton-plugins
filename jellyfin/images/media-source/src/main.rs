//! 入口：默认起服务；`healthcheck` 给镜像的健康检查用；`credentials issue|revoke <名字>` 是清单里
//! `provides.media_source.credentials` 的命令，值从标准输入读一行（规范“凭据的签发与吊销”）。镜像是
//! scratch，没有 shell，所以这几样都是同一个二进制的子命令。

use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use wonton_jellyfin_media::accounts::Accounts;
use wonton_jellyfin_media::jellyfin::Jellyfin;
use wonton_jellyfin_media::store::Credentials;
use wonton_jellyfin_media::{App, router};

const PORT: u16 = 8090;

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "/data".into()))
}

fn credentials() -> Credentials {
    Credentials::new(data_dir().join("credentials"))
}

fn credentials_command(args: &[String]) -> ExitCode {
    let (Some(op), Some(name)) = (args.first(), args.get(1)) else {
        eprintln!("用法：credentials issue|revoke <名字>");
        return ExitCode::from(2);
    };
    let result = match op.as_str() {
        "issue" => {
            let mut line = String::new();
            if let Err(e) = std::io::stdin().lock().read_line(&mut line) {
                eprintln!("读不到标准输入：{e}");
                return ExitCode::FAILURE;
            }
            credentials().issue(name, line.trim_end_matches(['\r', '\n']))
        }
        "revoke" => credentials().revoke(name),
        other => {
            eprintln!("不认识的凭据命令：{other}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("凭据 {op} 失败：{e}");
            ExitCode::FAILURE
        }
    }
}

/// 健康检查打本机 `/healthz`：它会去问 Jellyfin，管理员 key 不对时也算不健康。
fn healthcheck() -> ExitCode {
    use std::io::{Read, Write};
    let Ok(mut conn) = std::net::TcpStream::connect(("127.0.0.1", PORT)) else {
        return ExitCode::FAILURE;
    };
    let _ = conn.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    if conn
        .write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .is_err()
    {
        return ExitCode::FAILURE;
    }
    let mut head = [0u8; 12];
    match conn.read_exact(&mut head) {
        Ok(()) if head.starts_with(b"HTTP/1.") && &head[9..12] == b"204" => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("healthcheck") => return healthcheck(),
        Some("credentials") => return credentials_command(&args[1..]),
        Some(other) => {
            eprintln!("不认识的子命令：{other}");
            return ExitCode::from(2);
        }
        None => {}
    }
    let base = std::env::var("JELLYFIN_URL").unwrap_or_else(|_| "http://jellyfin:8096".into());
    let key = std::env::var("JELLYFIN_API_KEY").unwrap_or_default();
    if key.trim().is_empty() {
        eprintln!(
            "没有 Jellyfin 的 API key：在 Jellyfin 的 控制台 → API 密钥 里新建一把，填到插件详情页"
        );
        return ExitCode::FAILURE;
    }
    let jellyfin = Jellyfin::new(base);
    let app = Arc::new(App {
        accounts: Accounts::new(
            data_dir().join("users"),
            jellyfin.clone(),
            key.trim().to_owned(),
        ),
        credentials: credentials(),
        jellyfin,
    });
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", PORT)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("监听 {PORT} 失败：{e}");
            return ExitCode::FAILURE;
        }
    };
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("装得上 SIGTERM 处理");
    let served = axum::serve(listener, router(app)).with_graceful_shutdown(async move {
        tokio::select! { _ = shutdown => {}, _ = term.recv() => {} }
    });
    match served.await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("服务出错：{e}");
            ExitCode::FAILURE
        }
    }
}

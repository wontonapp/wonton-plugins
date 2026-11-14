//! 只读的共享文件夹 MCP 服务。工具在 [`tools`] 里，路径访问在 [`fs`] 里。

pub mod fs;
pub mod matcher;
pub mod output;
pub mod params;
pub mod report;
pub mod tools;

mod text;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

pub use fs::SHARES;
pub use tools::FilesServer;

use fs::FilesFs;

pub const DEFAULT_PORT: u16 = 8080;

/// 每个共享文件夹都能打开、能读目录才算健康。
pub fn health(root: &Path) -> Result<(), String> {
    let fs = FilesFs::new(root);
    let mut bad = Vec::new();
    for share in SHARES {
        match fs.open_dir(share, &[]) {
            Ok(dir) => match rustix::fs::Dir::read_from(&dir) {
                Ok(mut entries) => {
                    if let Some(Err(errno)) = entries.next() {
                        bad.push(format!("{share}：{errno}"));
                    }
                }
                Err(errno) => bad.push(format!("{share}：{errno}")),
            },
            Err(err) => bad.push(format!("{share}：{err}")),
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad.join("；"))
    }
}

pub fn port_from_env() -> Result<u16, String> {
    match std::env::var("PORT") {
        Ok(value) => value.parse().map_err(|_| format!("PORT={value} 不是端口")),
        Err(_) => Ok(DEFAULT_PORT),
    }
}

pub fn shares_root() -> std::path::PathBuf {
    std::env::var_os("SHARES_DIR")
        .map(Into::into)
        .unwrap_or_else(|| "/shares".into())
}

/// 起 Streamable HTTP 服务，`/mcp` 给总线，`/healthz` 给健康检查。
pub async fn serve(server: FilesServer, port: u16) -> std::io::Result<()> {
    let cancellation = CancellationToken::new();
    let config = StreamableHttpServerConfig::default()
        .with_cancellation_token(cancellation.child_token())
        // 核心经插件网络用服务名连进来；主机名校验交给网络隔离。
        .disable_allowed_hosts();
    let app = app(server, config);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("wonton-files 在 0.0.0.0:{port}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown(cancellation))
        .await
}

pub fn app(server: FilesServer, config: StreamableHttpServerConfig) -> Router {
    let handler = server.clone();
    let mcp = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new()
        .route("/healthz", get(healthz))
        .nest_service("/mcp", mcp)
        .with_state(Arc::new(server))
}

async fn healthz(State(server): State<Arc<FilesServer>>) -> (StatusCode, String) {
    let root = server.fs().root().to_owned();
    match tokio::task::spawn_blocking(move || health(&root)).await {
        Ok(Ok(())) => (StatusCode::OK, "ok".to_owned()),
        Ok(Err(reason)) => (StatusCode::SERVICE_UNAVAILABLE, reason),
        Err(err) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
    }
}

async fn shutdown(cancellation: CancellationToken) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let term = async {
        let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            std::future::pending::<()>().await;
            return;
        };
        signal.recv().await;
    };
    tokio::select! {
        _ = ctrl_c => {}
        _ = term => {}
    }
    cancellation.cancel();
}

/// 健康检查客户端：镜像里没有 curl，用二进制自己的这个子命令。
pub fn healthcheck(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    if stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut response = String::new();
    let _ = stream.take(4096).read_to_string(&mut response);
    response.contains(" 200 ")
}

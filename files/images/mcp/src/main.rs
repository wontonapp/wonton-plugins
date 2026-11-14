use std::process::ExitCode;

use wonton_files::{FilesServer, health, healthcheck, port_from_env, serve, shares_root};

#[tokio::main]
async fn main() -> ExitCode {
    let port = match port_from_env() {
        Ok(port) => port,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::FAILURE;
        }
    };

    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return if healthcheck(port) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    let root = shares_root();
    // 失败即停：缺一个共享文件夹就不启动，不带着半套目录跑。
    if let Err(reason) = health(&root) {
        eprintln!("共享文件夹不可用（{}）：{reason}", root.display());
        return ExitCode::FAILURE;
    }

    if let Err(err) = serve(FilesServer::new(&root), port).await {
        eprintln!("服务起不来：{err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

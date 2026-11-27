//! 没填管理员 key 时：插件要装得上（健康检查只看 Jellyfin 自己在不在），媒体源的接口如实回未就绪。

use axum::Router;
use axum::routing::get;
use wonton_jellyfin_media::jellyfin::Jellyfin;
use wonton_jellyfin_media::unconfigured;

/// 没有服务在听的地址：连上去就被拒。
const NOBODY: &str = "http://127.0.0.1:9";

async fn serve(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

#[tokio::test]
async fn media_routes_report_the_plugin_unavailable_until_a_key_is_filled() {
    let jellyfin = serve(Router::new().route("/health", get(|| async { "Healthy" }))).await;
    let adapter = serve(unconfigured(Jellyfin::new(jellyfin))).await;
    for path in ["/_wonton/capabilities", "/items?q=x", "/catalog/libraries"] {
        let resp = http().get(format!("{adapter}{path}")).send().await.unwrap();
        assert_eq!(resp.status(), 503, "{path}");
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["code"], "plugin_unavailable", "{path}");
    }
}

#[tokio::test]
async fn health_follows_jellyfin_itself_while_the_key_is_missing() {
    let jellyfin = serve(Router::new().route("/health", get(|| async { "Healthy" }))).await;
    let up = serve(unconfigured(Jellyfin::new(jellyfin))).await;
    let resp = http().get(format!("{up}/healthz")).send().await.unwrap();
    assert_eq!(resp.status(), 204);

    let down = serve(unconfigured(Jellyfin::new(NOBODY))).await;
    let resp = http().get(format!("{down}/healthz")).send().await.unwrap();
    assert_eq!(resp.status(), 503);
}

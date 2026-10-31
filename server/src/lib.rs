//! Light-Video 后端库：路由、状态、安全中间件均在此暴露，
//! 二进制入口（main.rs）只负责启动、后台任务与优雅退出。
//! 拆成 lib 也让集成测试能直接对 Router 发请求，无需起端口。

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod media;
pub mod models;
pub mod routes;
pub mod security;
pub mod state;
pub mod util;
pub mod ws;

use std::path::PathBuf;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

/// 纯 API + 媒体路由（含安全中间件），不含静态站点
pub fn api_router(state: AppState) -> Router {
    let st = state.clone();
    routes::build(state)
        .layer(axum::middleware::from_fn_with_state(
            st.clone(),
            security::csrf_guard,
        ))
        .layer(axum::middleware::from_fn(security::security_headers))
}

/// 生产入口：API + SPA 静态资源 + SPA 回退
///
/// 注意：axum 的 `layer` 只作用于「调用 layer 之前已注册」的路由，
/// 因此静态资源与 fallback 必须先挂载，最后再统一加安全中间件，
/// 否则首页会绕过 CSP / CSRF cookie 下发。
pub fn app(state: AppState, web_dir: PathBuf) -> Router {
    let index = web_dir.join("index.html");
    // nest_service 会剥离 /assets 前缀，静态文件才能正确映射到磁盘
    let assets = tower::ServiceBuilder::new()
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache"),
        ))
        .service(ServeDir::new(web_dir.join("assets")));

    let st = state.clone();
    routes::build(state)
        .nest_service("/assets", assets)
        .fallback_service(ServeFile::new(index))
        .layer(axum::middleware::from_fn_with_state(
            st.clone(),
            security::csrf_guard,
        ))
        .layer(axum::middleware::from_fn(security::security_headers))
}

pub mod auth_routes;
pub mod chat;
pub mod comments;
pub mod danmaku;
pub mod media_routes;
pub mod notifications;
pub mod social;
pub mod uploads;
pub mod users;
pub mod videos;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue};
use axum::response::Response;
use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::ws::ws_handler;

/// 给响应追加多条 Set-Cookie
pub fn append_cookies(mut resp: Response, cookies: Vec<String>) -> Response {
    for c in cookies {
        if let Ok(v) = HeaderValue::from_str(&c) {
            resp.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    resp
}

pub fn build(state: AppState) -> Router {
    let chunk_limit = (state.cfg.chunk_size + 1024 * 1024) as usize;
    let image_limit = (state.cfg.max_cover_bytes + 1024 * 1024) as usize;

    // axum 的 DefaultBodyLimit 默认只有 2MiB，大于它的请求会在提取器处直接 413。
    // 分片与图片上传必须单独放宽；JSON 接口保持 1MiB，避免大 body 占内存。
    let chunk_routes = Router::new()
        .route("/uploads/{id}/chunk/{idx}", put(uploads::put_chunk))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(chunk_limit));

    let image_routes = Router::new()
        .route("/uploads/cover", post(uploads::upload_cover))
        .route("/auth/avatar", post(auth_routes::update_avatar))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(image_limit));

    let api = Router::new()
        // ---------- 认证 ----------
        .route("/auth/register", post(auth_routes::register))
        .route("/auth/login", post(auth_routes::login))
        .route("/auth/logout", post(auth_routes::logout))
        .route("/auth/me", get(auth_routes::me))
        .route("/auth/password", post(auth_routes::change_password))
        .route("/auth/profile", patch(auth_routes::update_profile))
        // ---------- 视频 ----------
        .route("/videos", get(videos::list))
        .route(
            "/videos/{id}",
            get(videos::detail)
                .patch(videos::update)
                .delete(videos::remove),
        )
        .route("/videos/{id}/related", get(videos::related))
        .route(
            "/videos/{id}/progress",
            get(videos::get_progress).post(videos::save_progress),
        )
        .route("/me/favorites", get(videos::my_favorites))
        .route("/me/history", get(videos::my_history))
        .route("/videos/{id}/like", post(social::toggle_like))
        .route("/videos/{id}/favorite", post(social::toggle_favorite))
        .route("/videos/{id}/interaction", get(social::interaction))
        .route("/videos/{id}/share", post(social::share))
        .route(
            "/videos/{id}/comments",
            get(comments::list).post(comments::create),
        )
        .route(
            "/videos/{id}/danmaku",
            get(danmaku::list).post(danmaku::create),
        )
        .route("/comments/{id}", delete(comments::remove))
        .route("/danmaku/{id}", delete(danmaku::remove))
        // ---------- 分片上传 ----------
        .route("/uploads/init", post(uploads::init))
        .route("/uploads/{id}", get(uploads::status).delete(uploads::abort))
        .route("/uploads/{id}/complete", post(uploads::complete))
        // ---------- 用户与社交 ----------
        .route("/users/search", get(users::search))
        .route("/users/{id}", get(users::profile))
        .route("/friends", get(social::friends))
        .route("/friends/requests", get(social::requests))
        .route("/users/{id}/friend-request", post(social::friend_request))
        .route("/users/{id}/friend-accept", post(social::friend_accept))
        .route("/users/{id}/friend-reject", post(social::friend_reject))
        .route("/users/{id}/friend", delete(social::friend_remove))
        // ---------- 私信 ----------
        .route("/conversations", get(chat::conversations))
        .route("/messages", post(chat::send))
        .route("/messages/{user_id}", get(chat::history))
        .route("/messages/read", post(chat::mark_read))
        .route("/messages/unread", get(chat::unread_count))
        // ---------- 通知 ----------
        .route("/notifications", get(notifications::list))
        .route("/notifications/read", post(notifications::mark_read))
        .route("/notifications/unread", get(notifications::unread))
        // ---------- 系统 ----------
        .route("/meta", get(meta))
        .route("/health", get(health))
        .route("/stats", get(stats))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .merge(chunk_routes)
        .merge(image_routes)
        .layer(RequestBodyLimitLayer::new(chunk_limit.max(image_limit)))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            std::time::Duration::from_secs(120),
        ))
        .layer(CompressionLayer::new().br(true).gzip(true));

    Router::new()
        .nest("/api", api)
        .route("/ws", get(ws_handler))
        // 媒体文件：长缓存 + Range
        .route("/media/{*path}", get(media_routes::media))
        .route("/hls/{*path}", get(media_routes::hls))
        .route("/healthz", get(healthz))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn health(
    axum::extract::State(st): axum::extract::State<AppState>,
) -> axum::Json<serde_json::Value> {
    let db_ok = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&st.db)
        .await
        .is_ok();
    axum::Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "uptime_ms": crate::util::now_ms() - st.started_at,
        "db": db_ok,
        "ws_online": st.hub.online_users(),
        "ffmpeg": st.ffmpeg_available,
    }))
}

async fn stats(
    axum::extract::State(st): axum::extract::State<AppState>,
) -> crate::error::AppResult<axum::Json<crate::util::ApiOk<serde_json::Value>>> {
    let videos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM videos WHERE status='ready'")
        .fetch_one(&st.db)
        .await?;
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&st.db)
        .await?;
    let danmaku: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM danmaku WHERE is_deleted=0")
        .fetch_one(&st.db)
        .await?;
    let views: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(views),0) FROM videos")
        .fetch_one(&st.db)
        .await?;
    Ok(crate::util::ok(serde_json::json!({
        "videos": videos, "users": users, "danmaku": danmaku, "views": views,
        "online": st.hub.online_users(),
    })))
}

async fn meta() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "categories": [
            {"id": "anime", "name": "动画"},
            {"id": "game", "name": "游戏"},
            {"id": "music", "name": "音乐"},
            {"id": "tech", "name": "科技"},
            {"id": "life", "name": "生活"},
            {"id": "movie", "name": "影视"},
            {"id": "study", "name": "学习"},
            {"id": "other", "name": "其他"}
        ],
        "danmaku": {"max_length": 80, "modes": ["scroll", "top", "bottom"]},
        "limits": {"chunk_mb_hint": 4}
    }))
}

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::RequiredUser;
use crate::error::{AppError, AppResult};
use crate::models::{AuthUser, DanmakuRow};
use crate::security::client_ip;
use crate::state::AppState;
use crate::util::{clean_text, now_ms};

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub from_ms: Option<i64>,
    #[serde(default)]
    pub to_ms: Option<i64>,
}

pub async fn list(
    State(st): State<AppState>,
    Path(video_id): Path<i64>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let from = q.from_ms.unwrap_or(0).max(0);
    let to = q.to_ms.unwrap_or(i64::MAX / 2).max(from);
    // 单次最多 4000 条，防止长视频全量拉取拖垮首屏
    let rows = sqlx::query_as::<_, DanmakuRow>(
        "SELECT d.id, d.content, d.time_ms, d.color, d.mode, d.font_size, d.user_id, \
         COALESCE(NULLIF(u.display_name,''), u.username, '游客') AS username, \
         d.created_at AS created_at_ms, '' AS created_at \
         FROM danmaku d LEFT JOIN users u ON u.id = d.user_id \
         WHERE d.video_id = ? AND d.is_deleted = 0 AND d.time_ms BETWEEN ? AND ? \
         ORDER BY d.time_ms ASC LIMIT 4000",
    )
    .bind(video_id)
    .bind(from)
    .bind(to)
    .fetch_all(&st.db)
    .await?;

    let mut out = rows;
    for r in &mut out {
        r.created_at = crate::util::iso(r.created_at_ms);
    }
    Ok(Json(json!({"ok": true, "data": out})))
}

#[derive(Deserialize)]
pub struct CreateReq {
    pub content: String,
    #[serde(default)]
    pub time_ms: i64,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub font_size: Option<i64>,
}

/// HTTP 兜底发送（WS 不可用时）
pub async fn create(
    State(st): State<AppState>,
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: RequiredUser,
    Path(video_id): Path<i64>,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ip = client_ip(&headers, Some(ci.ip()), st.cfg.trust_proxy);
    st.rate_limit("danmaku", &format!("{}:{}", user.0.id, ip), 12, 60)?;

    let data = insert_danmaku(
        &st,
        &user.0,
        video_id,
        &req.content,
        req.time_ms,
        req.color,
        req.mode,
        req.font_size,
    )
    .await?;

    st.hub.broadcast_event(
        &format!("video:{video_id}"),
        &crate::ws::ServerEvent::Danmaku { data: data.clone() },
    );
    Ok(Json(json!({"ok": true, "data": data})))
}

/// 由 HTTP 与 WebSocket 共用：校验 → 落库 → 返回广播用 JSON
#[allow(clippy::too_many_arguments)]
pub async fn insert_danmaku(
    st: &AppState,
    user: &AuthUser,
    video_id: i64,
    content: &str,
    time_ms: i64,
    color: Option<String>,
    mode: Option<String>,
    font_size: Option<i64>,
) -> AppResult<serde_json::Value> {
    let content = clean_text(content, 80);
    if content.is_empty() {
        return Err(AppError::BadRequest("弹幕内容不能为空".into()));
    }

    let duration: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT duration_ms FROM videos WHERE id = ? AND status = 'ready'",
    )
    .bind(video_id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(|| AppError::NotFound("视频不存在".into()))?;

    let mut t = time_ms.max(0);
    if duration > 0 {
        t = t.min(duration);
    }

    let color = color
        .filter(|c| {
            c.len() == 7 && c.starts_with('#') && c.chars().skip(1).all(|ch| ch.is_ascii_hexdigit())
        })
        .unwrap_or_else(|| "#ffffff".into());

    let mode = match mode.as_deref() {
        Some("top") => "top",
        Some("bottom") => "bottom",
        _ => "scroll",
    };
    let font_size = font_size.unwrap_or(25).clamp(14, 48);

    let now = now_ms();
    let res = sqlx::query(
        "INSERT INTO danmaku (video_id, user_id, content, time_ms, color, mode, font_size, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(video_id)
    .bind(user.id)
    .bind(&content)
    .bind(t)
    .bind(&color)
    .bind(mode)
    .bind(font_size)
    .bind(now)
    .execute(&st.db)
    .await?;

    // 计数用原子自增，避免读-改-写竞态
    sqlx::query("UPDATE videos SET danmaku_count = danmaku_count + 1 WHERE id = ?")
        .bind(video_id)
        .execute(&st.db)
        .await?;

    Ok(json!({
        "id": res.last_insert_rowid(),
        "content": content,
        "time_ms": t,
        "color": color,
        "mode": mode,
        "font_size": font_size,
        "user_id": user.id,
        "username": user.username,
        "created_at": crate::util::iso(now),
    }))
}

pub async fn remove(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let row: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT d.user_id, d.video_id, v.user_id FROM danmaku d JOIN videos v ON v.id = d.video_id WHERE d.id = ?",
    )
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let (sender, video_id, owner) = row.ok_or_else(|| AppError::NotFound("弹幕不存在".into()))?;

    if user.0.id != sender && user.0.id != owner && !user.0.is_admin() {
        return Err(AppError::Forbidden("无权删除该弹幕".into()));
    }
    sqlx::query("UPDATE danmaku SET is_deleted = 1 WHERE id = ?")
        .bind(id)
        .execute(&st.db)
        .await?;
    sqlx::query("UPDATE videos SET danmaku_count = MAX(danmaku_count - 1, 0) WHERE id = ?")
        .bind(video_id)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({"ok": true, "data": {"deleted": true}})))
}

#[allow(dead_code)]
pub async fn count(state: &AppState, video_id: i64) -> AppResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM danmaku WHERE video_id = ? AND is_deleted = 0")
            .bind(video_id)
            .fetch_one(&state.db)
            .await?,
    )
}

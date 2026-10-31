use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{MaybeUser, RequiredUser};
use crate::error::{AppError, AppResult};
use crate::models::CommentRow;
use crate::security::client_ip;
use crate::state::AppState;
use crate::util::{clean_text, now_ms, page_bounds};

const COMMENT_SELECT: &str = "SELECT c.id, c.content, c.parent_id, c.likes_count, c.user_id, \
COALESCE(NULLIF(u.display_name,''), u.username, '已注销') AS username, u.avatar_key, \
'' AS avatar_url, c.created_at AS created_at_ms, '' AS created_at, 0 AS is_owner \
FROM comments c LEFT JOIN users u ON u.id = c.user_id";

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list(
    State(st): State<AppState>,
    MaybeUser(viewer): MaybeUser,
    Path(video_id): Path<i64>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let (limit, offset) = page_bounds(q.page, q.limit);
    let mut rows = sqlx::query_as::<_, CommentRow>(&format!(
        "{} WHERE c.video_id = ? AND c.is_deleted = 0 AND c.parent_id IS NULL \
         ORDER BY c.id DESC LIMIT ? OFFSET ?",
        COMMENT_SELECT
    ))
    .bind(video_id)
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let viewer_id = viewer.as_ref().map(|u| u.id);
    for r in &mut rows {
        r.fix(viewer_id);
    }

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM comments WHERE video_id = ? AND is_deleted = 0 AND parent_id IS NULL",
    )
    .bind(video_id)
    .fetch_one(&st.db)
    .await?;

    Ok(Json(json!({
        "ok": true,
        "data": {"items": rows, "page": q.page.unwrap_or(1), "limit": limit, "has_more": has_more, "total": total}
    })))
}

#[derive(Deserialize)]
pub struct CreateReq {
    pub content: String,
    #[serde(default)]
    pub parent_id: Option<i64>,
}

pub async fn create(
    State(st): State<AppState>,
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: RequiredUser,
    Path(video_id): Path<i64>,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ip = client_ip(&headers, Some(ci.ip()), st.cfg.trust_proxy);
    st.rate_limit("comment", &format!("{}:{}", user.0.id, ip), 12, 60)?;

    let content = clean_text(&req.content, 1000);
    if content.chars().count() < 1 {
        return Err(AppError::BadRequest("评论内容不能为空".into()));
    }

    let video: Option<(i64, String)> =
        sqlx::query_as("SELECT user_id, title FROM videos WHERE id = ? AND status = 'ready'")
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;
    let (owner_id, title) = video.ok_or_else(|| AppError::NotFound("视频不存在".into()))?;

    if let Some(pid) = req.parent_id {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM comments WHERE id = ? AND video_id = ? AND is_deleted = 0",
        )
        .bind(pid)
        .bind(video_id)
        .fetch_optional(&st.db)
        .await?;
        if exists.is_none() {
            return Err(AppError::BadRequest("回复的评论不存在".into()));
        }
    }

    let now = now_ms();
    let res = sqlx::query(
        "INSERT INTO comments (video_id, user_id, parent_id, content, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(video_id)
    .bind(user.0.id)
    .bind(req.parent_id)
    .bind(&content)
    .bind(now)
    .execute(&st.db)
    .await?;
    let cid = res.last_insert_rowid();

    sqlx::query("UPDATE videos SET comments_count = comments_count + 1 WHERE id = ?")
        .bind(video_id)
        .execute(&st.db)
        .await?;

    if owner_id != user.0.id {
        let text = if req.parent_id.is_some() {
            format!("{} 回复了你在《{}》下的评论", user.0.username, title)
        } else {
            format!("{} 评论了你的视频《{}》", user.0.username, title)
        };
        crate::routes::notifications::push(
            &st,
            owner_id,
            "comment",
            Some(user.0.id),
            Some(video_id),
            Some(cid),
            &text,
        )
        .await;
    }

    let mut row = sqlx::query_as::<_, CommentRow>(&format!("{} WHERE c.id = ?", COMMENT_SELECT))
        .bind(cid)
        .fetch_one(&st.db)
        .await?;
    row.fix(Some(user.0.id));

    Ok(Json(json!({"ok": true, "data": row})))
}

pub async fn remove(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let row: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT c.user_id, c.video_id, v.user_id FROM comments c JOIN videos v ON v.id = c.video_id WHERE c.id = ?",
    )
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let (author, video_id, owner) = row.ok_or_else(|| AppError::NotFound("评论不存在".into()))?;
    if user.0.id != author && user.0.id != owner && !user.0.is_admin() {
        return Err(AppError::Forbidden("无权删除该评论".into()));
    }
    sqlx::query("UPDATE comments SET is_deleted = 1, content = '[已删除]' WHERE id = ?")
        .bind(id)
        .execute(&st.db)
        .await?;
    sqlx::query("UPDATE videos SET comments_count = MAX(comments_count - 1, 0) WHERE id = ?")
        .bind(video_id)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({"ok": true, "data": {"deleted": true}})))
}

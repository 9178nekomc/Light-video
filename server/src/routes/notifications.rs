use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::RequiredUser;
use crate::error::AppResult;
use crate::models::NotificationRow;
use crate::state::AppState;
use crate::util::{now_ms, page_bounds};

/// 落库 + 尝试实时推送
pub async fn push(
    st: &AppState,
    user_id: i64,
    kind: &str,
    actor_id: Option<i64>,
    video_id: Option<i64>,
    comment_id: Option<i64>,
    content: &str,
) {
    let now = now_ms();
    let res = sqlx::query(
        "INSERT INTO notifications (user_id, kind, actor_id, video_id, comment_id, content, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(kind)
    .bind(actor_id)
    .bind(video_id)
    .bind(comment_id)
    .bind(content)
    .bind(now)
    .execute(&st.db)
    .await;

    if let Ok(r) = res {
        crate::ws::push_notification(
            &st.hub,
            user_id,
            json!({
                "id": r.last_insert_rowid(),
                "kind": kind,
                "content": content,
                "video_id": video_id,
                "actor_id": actor_id,
                "is_read": 0,
                "created_at": crate::util::iso(now),
            }),
        );
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub unread_only: Option<bool>,
}

pub async fn list(
    State(st): State<AppState>,
    user: RequiredUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let (limit, offset) = page_bounds(q.page, q.limit);
    let unread_only = q.unread_only.unwrap_or(false);

    let mut rows = sqlx::query_as::<_, NotificationRow>(
        "SELECT n.id, n.kind, n.content, n.video_id, n.actor_id, n.is_read, \
         n.created_at AS created_at_ms, '' AS created_at, \
         COALESCE(NULLIF(u.display_name,''), u.username) AS actor_name \
         FROM notifications n LEFT JOIN users u ON u.id = n.actor_id \
         WHERE n.user_id = ? AND (? = 0 OR n.is_read = 0) \
         ORDER BY n.id DESC LIMIT ? OFFSET ?",
    )
    .bind(user.0.id)
    .bind(if unread_only { 1 } else { 0 })
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(&st.db)
    .await?;

    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    for r in &mut rows {
        r.fix();
    }

    Ok(Json(json!({
        "ok": true,
        "data": {"items": rows, "page": q.page.unwrap_or(1), "limit": limit, "has_more": has_more}
    })))
}

pub async fn mark_read(
    State(st): State<AppState>,
    user: RequiredUser,
) -> AppResult<Json<serde_json::Value>> {
    let n = sqlx::query("UPDATE notifications SET is_read = 1 WHERE user_id = ? AND is_read = 0")
        .bind(user.0.id)
        .execute(&st.db)
        .await?
        .rows_affected();
    Ok(Json(json!({"ok": true, "data": {"updated": n}})))
}

pub async fn unread(
    State(st): State<AppState>,
    user: RequiredUser,
) -> AppResult<Json<serde_json::Value>> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = ? AND is_read = 0")
            .bind(user.0.id)
            .fetch_one(&st.db)
            .await?;
    Ok(Json(json!({"ok": true, "data": {"count": n}})))
}

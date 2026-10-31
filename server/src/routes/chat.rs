use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::RequiredUser;
use crate::error::{AppError, AppResult};
use crate::models::{AuthUser, ConversationRow, MessageRow};
use crate::state::AppState;
use crate::util::{clean_text, now_ms};
use crate::ws::{Hub, ServerEvent};

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default)]
    pub before_id: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn conversations(
    State(st): State<AppState>,
    user: RequiredUser,
) -> AppResult<Json<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct PeerRow {
        peer: i64,
        #[allow(dead_code)]
        mid: i64,
        content: String,
        created_at: i64,
        kind: String,
        username: String,
        display_name: String,
        avatar_key: String,
    }

    let pairs = sqlx::query_as::<_, PeerRow>(
        "SELECT t.peer, m.id AS mid, m.content, m.created_at, m.kind, u.username, u.display_name, u.avatar_key \
         FROM (SELECT CASE WHEN sender_id = ? THEN receiver_id ELSE sender_id END AS peer, MAX(id) AS mid \
               FROM messages WHERE sender_id = ? OR receiver_id = ? GROUP BY peer) t \
         JOIN messages m ON m.id = t.mid JOIN users u ON u.id = t.peer \
         ORDER BY m.id DESC LIMIT 80",
    )
    .bind(user.0.id)
    .bind(user.0.id)
    .bind(user.0.id)
    .fetch_all(&st.db)
    .await?;

    #[derive(sqlx::FromRow)]
    struct UnreadRow {
        sender_id: i64,
        n: i64,
    }
    let unread = sqlx::query_as::<_, UnreadRow>(
        "SELECT sender_id, COUNT(*) AS n FROM messages WHERE receiver_id = ? AND is_read = 0 GROUP BY sender_id",
    )
    .bind(user.0.id)
    .fetch_all(&st.db)
    .await?;

    let out: Vec<ConversationRow> = pairs
        .into_iter()
        .map(|p| {
            let mut row = ConversationRow {
                user_id: p.peer,
                username: if p.display_name.is_empty() {
                    p.username
                } else {
                    p.display_name
                },
                avatar_key: p.avatar_key,
                last_content: if p.kind == "video" {
                    format!("[视频] {}", p.content)
                } else {
                    p.content
                },
                last_at: p.created_at,
                unread: unread
                    .iter()
                    .find(|u| u.sender_id == p.peer)
                    .map(|u| u.n)
                    .unwrap_or(0),
                avatar_url: String::new(),
                last_time: String::new(),
            };
            row.fix();
            row
        })
        .collect();

    Ok(Json(json!({"ok": true, "data": out})))
}

pub async fn history(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(peer): Path<i64>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let before = q.before_id.unwrap_or(i64::MAX);

    let mut rows = sqlx::query_as::<_, MessageRow>(
        "SELECT id, sender_id, receiver_id, content, kind, video_id, is_read, created_at AS created_at_ms, '' AS created_at \
         FROM messages \
         WHERE id < ? AND ((sender_id = ? AND receiver_id = ?) OR (sender_id = ? AND receiver_id = ?)) \
         ORDER BY id DESC LIMIT ?",
    )
    .bind(before)
    .bind(user.0.id)
    .bind(peer)
    .bind(peer)
    .bind(user.0.id)
    .bind(limit + 1)
    .fetch_all(&st.db)
    .await?;

    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    rows.reverse();
    for r in &mut rows {
        r.fix();
    }

    // 打开会话即已读
    sqlx::query(
        "UPDATE messages SET is_read = 1 WHERE sender_id = ? AND receiver_id = ? AND is_read = 0",
    )
    .bind(peer)
    .bind(user.0.id)
    .execute(&st.db)
    .await?;

    Ok(Json(json!({
        "ok": true,
        "data": {"items": rows, "has_more": has_more, "peer": peer}
    })))
}

#[derive(Deserialize)]
pub struct SendReq {
    pub to: i64,
    pub content: String,
    #[serde(default)]
    pub video_id: Option<i64>,
}

pub async fn send(
    State(st): State<AppState>,
    user: RequiredUser,
    Json(req): Json<SendReq>,
) -> AppResult<Json<serde_json::Value>> {
    st.rate_limit("dm", &user.0.id.to_string(), 60, 60)?;
    let msg = send_dm(&st, &st.hub, &user.0, req.to, &req.content, req.video_id).await?;
    Ok(Json(json!({"ok": true, "data": msg})))
}

async fn are_friends(st: &AppState, a: i64, b: i64) -> bool {
    if a == b {
        return false;
    }
    sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM friendships WHERE status = 'accepted' AND \
         ((requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?))",
    )
    .bind(a)
    .bind(b)
    .bind(b)
    .bind(a)
    .fetch_optional(&st.db)
    .await
    .ok()
    .flatten()
    .is_some()
}

/// HTTP 与 WebSocket 共用
pub async fn send_dm(
    st: &AppState,
    hub: &Hub,
    sender: &AuthUser,
    to: i64,
    content: &str,
    video_id: Option<i64>,
) -> AppResult<MessageRow> {
    let content = clean_text(content, 2000);
    if content.is_empty() {
        return Err(AppError::BadRequest("消息内容不能为空".into()));
    }
    if to == sender.id {
        return Err(AppError::BadRequest("不能给自己发消息".into()));
    }
    if !are_friends(st, sender.id, to).await {
        return Err(AppError::Forbidden("你们还不是好友".into()));
    }

    let (kind, video_title) = match video_id {
        Some(vid) => {
            let title: Option<String> = sqlx::query_scalar("SELECT title FROM videos WHERE id = ?")
                .bind(vid)
                .fetch_optional(&st.db)
                .await?;
            let title = title.ok_or_else(|| AppError::NotFound("视频不存在".into()))?;
            ("video", Some(title))
        }
        None => ("text", None),
    };

    let now = now_ms();
    let res = sqlx::query(
        "INSERT INTO messages (sender_id, receiver_id, content, kind, video_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(sender.id)
    .bind(to)
    .bind(&content)
    .bind(kind)
    .bind(video_id)
    .bind(now)
    .execute(&st.db)
    .await?;

    let mut msg = MessageRow {
        id: res.last_insert_rowid(),
        sender_id: sender.id,
        receiver_id: to,
        content,
        kind: kind.into(),
        video_id,
        is_read: 0,
        created_at_ms: now,
        created_at: String::new(),
    };
    msg.fix();

    // 在线则实时推送；离线才写通知，避免刷屏
    let delivered = hub.send_to_user(
        to,
        &ServerEvent::Chat {
            data: json!({
                "id": msg.id,
                "sender_id": sender.id,
                "sender_name": sender.username,
                "receiver_id": to,
                "content": msg.content,
                "kind": msg.kind,
                "video_id": msg.video_id,
                "created_at": msg.created_at,
            }),
        },
    );

    if !delivered {
        crate::routes::notifications::push(
            st,
            to,
            "message",
            Some(sender.id),
            video_id,
            None,
            &format!("{} 给你发了一条私信", sender.username),
        )
        .await;
    }

    if let Some(t) = video_title {
        let _ = t;
    }
    Ok(msg)
}

#[derive(Deserialize)]
pub struct ReadReq {
    pub user_id: i64,
}

pub async fn mark_read(
    State(st): State<AppState>,
    user: RequiredUser,
    Json(req): Json<ReadReq>,
) -> AppResult<Json<serde_json::Value>> {
    let n = sqlx::query(
        "UPDATE messages SET is_read = 1 WHERE sender_id = ? AND receiver_id = ? AND is_read = 0",
    )
    .bind(req.user_id)
    .bind(user.0.id)
    .execute(&st.db)
    .await?
    .rows_affected();
    Ok(Json(json!({"ok": true, "data": {"updated": n}})))
}

pub async fn unread_count(
    State(st): State<AppState>,
    user: RequiredUser,
) -> AppResult<Json<serde_json::Value>> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE receiver_id = ? AND is_read = 0")
            .bind(user.0.id)
            .fetch_one(&st.db)
            .await?;
    Ok(Json(json!({"ok": true, "data": {"count": n}})))
}

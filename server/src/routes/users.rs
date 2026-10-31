use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::MaybeUser;
use crate::error::{AppError, AppResult};
use crate::models::{UserRow, VideoCard, USER_COLS, VIDEO_COLS};
use crate::routes::videos::CARD_SELECT;
use crate::state::AppState;
use crate::util::{clean_text, escape_like_owned, iso, now_ms};

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

pub async fn search(
    State(st): State<AppState>,
    MaybeUser(viewer): MaybeUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let term = clean_text(&q.q, 40);
    if term.is_empty() {
        return Ok(Json(json!({"ok": true, "data": []})));
    }
    st.rate_limit(
        "user_search",
        &viewer
            .as_ref()
            .map(|u| u.id.to_string())
            .unwrap_or_else(|| "anon".into()),
        60,
        60,
    )?;

    let pattern = format!("%{}%", escape_like_owned(&term));
    let rows = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLS} FROM users \
         WHERE is_banned = 0 AND id != ? AND (username LIKE ? ESCAPE '\\' OR display_name LIKE ? ESCAPE '\\') \
         ORDER BY username COLLATE NOCASE LIMIT 12"
    ))
    .bind(viewer.as_ref().map(|u| u.id).unwrap_or(0))
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(&st.db)
    .await?;

    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|u| {
            let mut p = u.public();
            if p.display_name.is_empty() {
                p.display_name = p.username.clone();
            }
            json!({
                "id": p.id,
                "username": p.username,
                "display_name": p.display_name,
                "avatar_url": p.avatar_url,
                "bio": p.bio,
            })
        })
        .collect();

    Ok(Json(json!({"ok": true, "data": out})))
}

pub async fn profile(
    State(st): State<AppState>,
    MaybeUser(viewer): MaybeUser,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let user = sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLS} FROM users WHERE id = ?"))
        .bind(id)
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;

    let videos = sqlx::query_as::<_, VideoCard>(&format!(
        "{} WHERE v.user_id = ? AND v.status = 'ready' AND (v.visibility = 'public' OR v.user_id = ?) \
         ORDER BY v.created_at DESC LIMIT 24",
        CARD_SELECT
    ))
    .bind(id)
    .bind(viewer.as_ref().map(|u| u.id).unwrap_or(-1))
    .fetch_all(&st.db)
    .await?
    .into_iter()
    .map(|mut v| {
        v.fix();
        v
    })
    .collect::<Vec<_>>();

    let total_views: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(views),0) FROM videos WHERE user_id = ?")
            .bind(id)
            .fetch_one(&st.db)
            .await?;
    let likes: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(likes_count),0) FROM videos WHERE user_id = ?")
            .bind(id)
            .fetch_one(&st.db)
            .await?;
    let friends: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM friendships WHERE status='accepted' AND (requester_id = ? OR addressee_id = ?)",
    )
    .bind(id)
    .bind(id)
    .fetch_one(&st.db)
    .await?;

    let (is_friend, has_pending, can_request) = match viewer.as_ref() {
        Some(v) if v.id == id => (false, false, false),
        Some(v) => {
            #[derive(sqlx::FromRow)]
            struct Row {
                status: String,
                #[allow(dead_code)]
                requester_id: i64,
            }
            let rel = sqlx::query_as::<_, Row>(
                "SELECT status, requester_id FROM friendships WHERE (requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?)",
            )
            .bind(v.id)
            .bind(id)
            .bind(id)
            .bind(v.id)
            .fetch_optional(&st.db)
            .await?;
            match rel {
                Some(r) if r.status == "accepted" => (true, false, false),
                Some(r) if r.status == "pending" => (false, true, false),
                _ => (false, false, true),
            }
        }
        None => (false, false, false),
    };

    let mut pub_user = user.public();
    if pub_user.display_name.is_empty() {
        pub_user.display_name = pub_user.username.clone();
    }

    Ok(Json(json!({
        "ok": true,
        "data": {
            "user": pub_user,
            "stats": {"videos": videos.len(), "views": total_views, "likes": likes, "friends": friends},
            "videos": videos,
            "relation": {"is_friend": is_friend, "has_pending": has_pending, "can_request": can_request},
            "is_self": viewer.as_ref().map(|v| v.id) == Some(id),
            "joined_at": iso(user.created_at),
            "online": st.hub.presence(&format!("user:{id}")) > 0,
            "server_time": now_ms(),
        }
    })))
}

#[allow(dead_code)]
pub const _COLS: &str = VIDEO_COLS;

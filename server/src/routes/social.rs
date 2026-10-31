use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;

use crate::auth::RequiredUser;
use crate::error::{AppError, AppResult};
use crate::routes::notifications::push;
use crate::state::AppState;
use crate::util::{iso, now_ms};

async fn recount(st: &AppState, table: &str, column: &str, video_id: i64) -> AppResult<i64> {
    let sql = format!("UPDATE videos SET {column} = (SELECT COUNT(*) FROM {table} WHERE video_id = ?) WHERE id = ?");
    sqlx::query(&sql)
        .bind(video_id)
        .bind(video_id)
        .execute(&st.db)
        .await?;
    let n: i64 = sqlx::query_scalar(&format!("SELECT {column} FROM videos WHERE id = ?"))
        .bind(video_id)
        .fetch_one(&st.db)
        .await?;
    Ok(n)
}

pub async fn toggle_like(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(video_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    st.rate_limit("like", &user.0.id.to_string(), 120, 60)?;

    let video: Option<(i64, String)> =
        sqlx::query_as("SELECT user_id, title FROM videos WHERE id = ? AND status = 'ready'")
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;
    let (owner, title) = video.ok_or_else(|| AppError::NotFound("视频不存在".into()))?;

    let existing: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM likes WHERE user_id = ? AND video_id = ?")
            .bind(user.0.id)
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;

    let liked = if existing.is_some() {
        sqlx::query("DELETE FROM likes WHERE user_id = ? AND video_id = ?")
            .bind(user.0.id)
            .bind(video_id)
            .execute(&st.db)
            .await?;
        false
    } else {
        sqlx::query("INSERT OR IGNORE INTO likes (user_id, video_id, created_at) VALUES (?, ?, ?)")
            .bind(user.0.id)
            .bind(video_id)
            .bind(now_ms())
            .execute(&st.db)
            .await?;
        true
    };

    let count = recount(&st, "likes", "likes_count", video_id).await?;

    if liked && owner != user.0.id {
        push(
            &st,
            owner,
            "like",
            Some(user.0.id),
            Some(video_id),
            None,
            &format!("{} 点赞了你的视频《{}》", user.0.username, title),
        )
        .await;
    }

    Ok(Json(
        json!({"ok": true, "data": {"liked": liked, "likes_count": count}}),
    ))
}

pub async fn toggle_favorite(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(video_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    st.rate_limit("favorite", &user.0.id.to_string(), 120, 60)?;

    let video: Option<(i64, String)> =
        sqlx::query_as("SELECT user_id, title FROM videos WHERE id = ? AND status = 'ready'")
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;
    let (owner, title) = video.ok_or_else(|| AppError::NotFound("视频不存在".into()))?;

    let existing: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM favorites WHERE user_id = ? AND video_id = ?")
            .bind(user.0.id)
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;

    let favorited = if existing.is_some() {
        sqlx::query("DELETE FROM favorites WHERE user_id = ? AND video_id = ?")
            .bind(user.0.id)
            .bind(video_id)
            .execute(&st.db)
            .await?;
        false
    } else {
        sqlx::query(
            "INSERT OR IGNORE INTO favorites (user_id, video_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(user.0.id)
        .bind(video_id)
        .bind(now_ms())
        .execute(&st.db)
        .await?;
        true
    };

    let count = recount(&st, "favorites", "favorites_count", video_id).await?;

    if favorited && owner != user.0.id {
        push(
            &st,
            owner,
            "favorite",
            Some(user.0.id),
            Some(video_id),
            None,
            &format!("{} 收藏了你的视频《{}》", user.0.username, title),
        )
        .await;
    }

    Ok(Json(
        json!({"ok": true, "data": {"favorited": favorited, "favorites_count": count}}),
    ))
}

pub async fn interaction(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(video_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let liked: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM likes WHERE user_id = ? AND video_id = ?")
            .bind(user.0.id)
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;
    let favorited: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM favorites WHERE user_id = ? AND video_id = ?")
            .bind(user.0.id)
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;
    Ok(Json(json!({
        "ok": true,
        "data": {"liked": liked.is_some(), "favorited": favorited.is_some()}
    })))
}

pub async fn share(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(video_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM videos WHERE id = ?")
        .bind(video_id)
        .fetch_optional(&st.db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("视频不存在".into()));
    }
    sqlx::query("UPDATE videos SET shares_count = shares_count + 1 WHERE id = ?")
        .bind(video_id)
        .execute(&st.db)
        .await?;
    let _ = user;
    Ok(Json(json!({
        "ok": true,
        "data": {"url": format!("{}/watch/{}", st.cfg.public_url, video_id)}
    })))
}

// ==================== 好友 ====================

pub async fn friend_request(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(target): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    st.rate_limit("friend_req", &user.0.id.to_string(), 20, 3600)?;
    if target == user.0.id {
        return Err(AppError::BadRequest("不能添加自己为好友".into()));
    }
    let target_name: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
        .bind(target)
        .fetch_optional(&st.db)
        .await?;
    let target_name = target_name.ok_or_else(|| AppError::NotFound("用户不存在".into()))?;

    let existing: Option<(String, i64)> = sqlx::query_as(
        "SELECT status, requester_id FROM friendships WHERE (requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?)",
    )
    .bind(user.0.id)
    .bind(target)
    .bind(target)
    .bind(user.0.id)
    .fetch_optional(&st.db)
    .await?;

    match existing {
        Some((status, _)) if status == "accepted" => {
            return Err(AppError::Conflict("你们已经是好友了".into()))
        }
        Some((status, requester)) if status == "pending" => {
            // 对方已经发过请求 → 直接成为好友
            if requester == target {
                accept_pair(&st, user.0.id, target).await?;
                return Ok(Json(json!({"ok": true, "data": {"status": "accepted"}})));
            }
            return Err(AppError::Conflict("好友请求已发送，等待对方确认".into()));
        }
        Some(_) => {
            sqlx::query(
                "UPDATE friendships SET requester_id = ?, addressee_id = ?, status = 'pending', updated_at = ? \
                 WHERE (requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?)",
            )
            .bind(user.0.id)
            .bind(target)
            .bind(now_ms())
            .bind(user.0.id)
            .bind(target)
            .bind(target)
            .bind(user.0.id)
            .execute(&st.db)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO friendships (requester_id, addressee_id, status, created_at, updated_at) VALUES (?, ?, 'pending', ?, ?)",
            )
            .bind(user.0.id)
            .bind(target)
            .bind(now_ms())
            .bind(now_ms())
            .execute(&st.db)
            .await?;
        }
    }

    push(
        &st,
        target,
        "friend_request",
        Some(user.0.id),
        None,
        None,
        &format!("{} 请求添加你为好友", user.0.username),
    )
    .await;

    let _ = target_name;
    Ok(Json(json!({"ok": true, "data": {"status": "pending"}})))
}

async fn accept_pair(st: &AppState, me: i64, other: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE friendships SET status = 'accepted', updated_at = ? \
         WHERE status = 'pending' AND ((requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?))",
    )
    .bind(now_ms())
    .bind(me)
    .bind(other)
    .bind(other)
    .bind(me)
    .execute(&st.db)
    .await?;
    Ok(())
}

pub async fn friend_accept(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(target): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let pending: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM friendships WHERE requester_id = ? AND addressee_id = ? AND status = 'pending'",
    )
    .bind(target)
    .bind(user.0.id)
    .fetch_optional(&st.db)
    .await?;
    if pending.is_none() {
        return Err(AppError::NotFound("没有待处理的好友请求".into()));
    }
    accept_pair(&st, user.0.id, target).await?;
    push(
        &st,
        target,
        "friend_accept",
        Some(user.0.id),
        None,
        None,
        &format!("{} 接受了你的好友请求", user.0.username),
    )
    .await;
    Ok(Json(json!({"ok": true, "data": {"status": "accepted"}})))
}

pub async fn friend_reject(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(target): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    sqlx::query(
        "UPDATE friendships SET status = 'rejected', updated_at = ? \
         WHERE requester_id = ? AND addressee_id = ? AND status = 'pending'",
    )
    .bind(now_ms())
    .bind(target)
    .bind(user.0.id)
    .execute(&st.db)
    .await?;
    Ok(Json(json!({"ok": true, "data": {"status": "rejected"}})))
}

pub async fn friend_remove(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(target): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    sqlx::query(
        "DELETE FROM friendships WHERE (requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?)",
    )
    .bind(user.0.id)
    .bind(target)
    .bind(target)
    .bind(user.0.id)
    .execute(&st.db)
    .await?;
    Ok(Json(json!({"ok": true, "data": {"removed": true}})))
}

pub async fn friends(
    State(st): State<AppState>,
    user: RequiredUser,
) -> AppResult<Json<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        username: String,
        display_name: String,
        avatar_key: String,
        bio: String,
        created_at: i64,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT u.id, u.username, u.display_name, u.avatar_key, u.bio, u.created_at \
         FROM friendships f JOIN users u ON u.id = CASE WHEN f.requester_id = ? THEN f.addressee_id ELSE f.requester_id END \
         WHERE f.status = 'accepted' AND (f.requester_id = ? OR f.addressee_id = ?) \
         ORDER BY u.username COLLATE NOCASE",
    )
    .bind(user.0.id)
    .bind(user.0.id)
    .bind(user.0.id)
    .fetch_all(&st.db)
    .await?;

    let out: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "username": r.username,
                "display_name": if r.display_name.is_empty() { r.username.clone() } else { r.display_name },
                "avatar_url": if r.avatar_key.is_empty() { String::new() } else { format!("/media/{}", r.avatar_key) },
                "bio": r.bio,
                "online": st.hub.presence(&format!("user:{}", r.id)) > 0,
                "created_at": iso(r.created_at),
            })
        })
        .collect();

    Ok(Json(json!({"ok": true, "data": out})))
}

pub async fn requests(
    State(st): State<AppState>,
    user: RequiredUser,
) -> AppResult<Json<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        username: String,
        display_name: String,
        avatar_key: String,
        created_at: i64,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT u.id, u.username, u.display_name, u.avatar_key, f.created_at \
         FROM friendships f JOIN users u ON u.id = f.requester_id \
         WHERE f.addressee_id = ? AND f.status = 'pending' ORDER BY f.created_at DESC",
    )
    .bind(user.0.id)
    .fetch_all(&st.db)
    .await?;

    let out: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "username": r.username,
                "display_name": if r.display_name.is_empty() { r.username.clone() } else { r.display_name },
                "avatar_url": if r.avatar_key.is_empty() { String::new() } else { format!("/media/{}", r.avatar_key) },
                "created_at": iso(r.created_at),
            })
        })
        .collect();
    Ok(Json(json!({"ok": true, "data": out})))
}

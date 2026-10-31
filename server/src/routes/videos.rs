use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{MaybeUser, RequiredUser};
use crate::error::{AppError, AppResult};
use crate::models::{RenditionRow, VideoCard, VideoDetail, VideoRow, VIDEO_COLS};
use crate::state::AppState;
use crate::util::{
    clean_text, json_str_array, now_ms, ok, page_bounds, parse_str_array, valid_storage_key,
};

pub const CARD_SELECT: &str = "SELECT v.id, v.title, v.poster_key, v.duration_ms, v.views, \
v.likes_count, v.favorites_count, v.danmaku_count, v.comments_count, v.category, v.created_at AS created_at_ms, \
u.id AS author_id, COALESCE(NULLIF(u.display_name, ''), u.username) AS author_name, \
u.avatar_key AS author_avatar_key, '' AS poster_url, '' AS author_avatar, '' AS created_at \
FROM videos v JOIN users u ON u.id = v.user_id";

pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub feed: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub user_id: Option<i64>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list(
    State(st): State<AppState>,
    MaybeUser(viewer): MaybeUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<crate::util::ApiOk<crate::util::Page<VideoCard>>>> {
    let (limit, offset) = page_bounds(q.page, q.limit);
    let feed = q.feed.as_deref().unwrap_or("latest");

    let mut sql = String::from(CARD_SELECT);
    sql.push_str(" WHERE v.status = 'ready' AND v.visibility = 'public'");

    if let Some(uid) = q.user_id {
        sql.push_str(" AND v.user_id = ");
        sql.push_str(&uid.to_string());
    }
    if let Some(cat) = q
        .category
        .as_deref()
        .filter(|c| !c.is_empty() && *c != "all")
    {
        sql.push_str(&format!(" AND v.category = '{}'", cat.replace('\'', "")));
    }
    if let Some(qs) =
        q.q.as_deref()
            .map(|s| clean_text(s, 60))
            .filter(|s| !s.is_empty())
    {
        let esc = escape_like(&qs);
        sql.push_str(&format!(
            " AND (v.title LIKE '%{0}%' ESCAPE '\\' OR v.description LIKE '%{0}%' ESCAPE '\\')",
            esc
        ));
    }

    if feed == "following" {
        match viewer {
            Some(u) => {
                sql.push_str(&format!(
                    " AND v.user_id IN (SELECT CASE WHEN requester_id = {0} THEN addressee_id ELSE requester_id END \
                     FROM friendships WHERE status = 'accepted' AND (requester_id = {0} OR addressee_id = {0}))",
                    u.id
                ));
            }
            None => {
                return Ok(ok(crate::util::page(Vec::new(), 1, limit, false)));
            }
        }
    }

    match feed {
        "hot" => sql.push_str(
            " ORDER BY (v.views + v.likes_count * 6 + v.danmaku_count * 2) DESC, v.id DESC",
        ),
        "oldest" => sql.push_str(" ORDER BY v.created_at ASC"),
        _ => sql.push_str(" ORDER BY v.created_at DESC"),
    }
    sql.push_str(&format!(" LIMIT {} OFFSET {}", limit + 1, offset));

    let mut rows = sqlx::query_as::<_, VideoCard>(&sql)
        .fetch_all(&st.db)
        .await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    for r in &mut rows {
        r.fix();
    }
    Ok(ok(crate::util::page(
        rows,
        q.page.unwrap_or(1).max(1),
        limit,
        has_more,
    )))
}

pub async fn detail(
    State(st): State<AppState>,
    MaybeUser(viewer): MaybeUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let v = fetch_video(&st, id).await?;
    let is_owner = viewer.as_ref().map(|u| u.id) == Some(v.user_id);
    let is_admin = viewer.as_ref().map(|u| u.is_admin()).unwrap_or(false);

    if v.status != "ready" && !is_owner && !is_admin {
        return Err(AppError::NotFound("视频不存在".into()));
    }
    if v.visibility == "private" && !is_owner && !is_admin {
        return Err(AppError::Forbidden("该视频为私密视频".into()));
    }

    // 播放计数：同一客户端 30 分钟内只计一次（防刷）
    let client = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(48)
        .collect::<String>();
    let fp = crate::security::sha256_hex(&format!(
        "{}|{}",
        viewer
            .as_ref()
            .map(|u| u.id.to_string())
            .unwrap_or_default(),
        client
    ));
    if st.views.hit(id, &fp[..16], 1800) {
        // 内存累加，后台批量落库
    }

    let author = sqlx::query_as::<_, crate::models::UserRow>(&format!(
        "SELECT {} FROM users WHERE id = ?",
        crate::models::USER_COLS
    ))
    .bind(v.user_id)
    .fetch_one(&st.db)
    .await?;

    let (liked, favorited, following) = match viewer.as_ref() {
        Some(u) => {
            let liked: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM likes WHERE user_id = ? AND video_id = ?")
                    .bind(u.id)
                    .bind(id)
                    .fetch_optional(&st.db)
                    .await?;
            let fav: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM favorites WHERE user_id = ? AND video_id = ?")
                    .bind(u.id)
                    .bind(id)
                    .fetch_optional(&st.db)
                    .await?;
            let follow: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM friendships WHERE status='accepted' AND \
                 ((requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?))",
            )
            .bind(u.id)
            .bind(v.user_id)
            .bind(v.user_id)
            .bind(u.id)
            .fetch_optional(&st.db)
            .await?;
            (liked.is_some(), fav.is_some(), follow.is_some())
        }
        None => (false, false, false),
    };

    let mut card = VideoCard {
        id: v.id,
        title: v.title.clone(),
        poster_key: v.poster_key.clone(),
        duration_ms: v.duration_ms,
        views: v.views,
        likes_count: v.likes_count,
        favorites_count: v.favorites_count,
        danmaku_count: v.danmaku_count,
        comments_count: v.comments_count,
        category: v.category.clone(),
        created_at_ms: v.created_at,
        author_id: author.id,
        author_name: if author.display_name.is_empty() {
            author.username.clone()
        } else {
            author.display_name.clone()
        },
        author_avatar_key: author.avatar_key.clone(),
        poster_url: String::new(),
        author_avatar: String::new(),
        created_at: String::new(),
    };
    card.fix();

    let renditions = sqlx::query_as::<_, RenditionRow>(
        "SELECT name, width, height, bitrate_kbps, playlist_key, '' AS url FROM video_renditions WHERE video_id = ? ORDER BY height ASC",
    )
    .bind(id)
    .fetch_all(&st.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|mut r| {
        r.url = format!("/hls/{}", r.playlist_key);
        r
    })
    .collect();

    let detail = VideoDetail {
        card,
        description: v.description.clone(),
        tags: parse_str_array(&v.tags),
        status: v.status.clone(),
        visibility: v.visibility.clone(),
        width: v.width,
        height: v.height,
        size_bytes: v.size_bytes,
        mime: v.mime.clone(),
        stream_url: format!("/media/{}", v.media_key),
        hls_url: if v.hls_key.is_empty() {
            String::new()
        } else {
            format!("/hls/{}", v.hls_key)
        },
        renditions,
        liked,
        favorited,
        following,
        is_owner,
    };

    Ok(Json(json!({"ok": true, "data": detail})))
}

pub async fn related(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    // 没有推荐模型，也不该假装有：直接随机抽。
    // 注意 RANDOM() 需要扫描候选集，视频量到十万级时应改为「随机 id + 范围查询」。
    let mut rows = sqlx::query_as::<_, VideoCard>(&format!(
        "{} WHERE v.status = 'ready' AND v.visibility = 'public' AND v.id != ? \
         ORDER BY RANDOM() LIMIT 10",
        CARD_SELECT
    ))
    .bind(id)
    .fetch_all(&st.db)
    .await?;

    for r in &mut rows {
        r.fix();
    }
    Ok(Json(json!({"ok": true, "data": rows})))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub visibility: Option<String>,
}

pub async fn update(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<serde_json::Value>> {
    let v = fetch_video(&st, id).await?;
    if v.user_id != user.0.id && !user.0.is_admin() {
        return Err(AppError::Forbidden("无权修改该视频".into()));
    }

    if let Some(t) = req.title {
        let t = clean_text(&t, 100);
        if t.is_empty() {
            return Err(AppError::BadRequest("标题不能为空".into()));
        }
        sqlx::query("UPDATE videos SET title = ?, updated_at = ? WHERE id = ?")
            .bind(&t)
            .bind(now_ms())
            .bind(id)
            .execute(&st.db)
            .await?;
    }
    if let Some(d) = req.description {
        sqlx::query("UPDATE videos SET description = ?, updated_at = ? WHERE id = ?")
            .bind(clean_text(&d, 5000))
            .bind(now_ms())
            .bind(id)
            .execute(&st.db)
            .await?;
    }
    if let Some(c) = req.category {
        const ALLOWED: &[&str] = &[
            "anime", "game", "music", "tech", "life", "movie", "study", "other",
        ];
        if !ALLOWED.contains(&c.as_str()) {
            return Err(AppError::BadRequest("无效分区".into()));
        }
        sqlx::query("UPDATE videos SET category = ?, updated_at = ? WHERE id = ?")
            .bind(&c)
            .bind(now_ms())
            .bind(id)
            .execute(&st.db)
            .await?;
    }
    if let Some(tags) = req.tags {
        let mut clean: Vec<String> = tags
            .into_iter()
            .map(|t| clean_text(&t, 16))
            .filter(|t| !t.is_empty())
            .take(8)
            .collect();
        clean.dedup();
        sqlx::query("UPDATE videos SET tags = ?, updated_at = ? WHERE id = ?")
            .bind(json_str_array(&clean))
            .bind(now_ms())
            .bind(id)
            .execute(&st.db)
            .await?;
    }
    if let Some(vis) = req.visibility {
        if !["public", "unlisted", "private"].contains(&vis.as_str()) {
            return Err(AppError::BadRequest("无效可见性".into()));
        }
        sqlx::query("UPDATE videos SET visibility = ?, updated_at = ? WHERE id = ?")
            .bind(&vis)
            .bind(now_ms())
            .bind(id)
            .execute(&st.db)
            .await?;
    }

    Ok(Json(json!({"ok": true, "data": {"updated": true}})))
}

pub async fn remove(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let v = fetch_video(&st, id).await?;
    if v.user_id != user.0.id && !user.0.is_admin() {
        return Err(AppError::Forbidden("无权删除该视频".into()));
    }

    sqlx::query("DELETE FROM videos WHERE id = ?")
        .bind(id)
        .execute(&st.db)
        .await?;

    // 异步删除磁盘资源，避免阻塞响应
    let cfg = st.cfg.clone();
    let media_key = v.media_key.clone();
    let poster_key = v.poster_key.clone();
    tokio::spawn(async move {
        for key in [media_key, poster_key] {
            if valid_storage_key(&key) {
                let _ = tokio::fs::remove_file(cfg.data_dir.join(&key)).await;
            }
        }
        let _ = tokio::fs::remove_dir_all(cfg.hls_dir().join(id.to_string())).await;
    });

    Ok(Json(json!({"ok": true, "data": {"deleted": true}})))
}

#[derive(Deserialize)]
pub struct ProgressReq {
    pub position_ms: i64,
}

pub async fn save_progress(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(id): Path<i64>,
    Json(req): Json<ProgressReq>,
) -> AppResult<Json<serde_json::Value>> {
    if req.position_ms < 0 {
        return Err(AppError::BadRequest("无效的进度".into()));
    }
    sqlx::query(
        "INSERT INTO watch_history (user_id, video_id, position_ms, updated_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(user_id, video_id) DO UPDATE SET position_ms = excluded.position_ms, updated_at = excluded.updated_at",
    )
    .bind(user.0.id)
    .bind(id)
    .bind(req.position_ms)
    .bind(now_ms())
    .execute(&st.db)
    .await?;
    Ok(Json(json!({"ok": true, "data": {"saved": true}})))
}

pub async fn get_progress(
    State(st): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let Some(u) = user else {
        return Ok(Json(json!({"ok": true, "data": {"position_ms": 0}})));
    };
    let pos: Option<i64> = sqlx::query_scalar(
        "SELECT position_ms FROM watch_history WHERE user_id = ? AND video_id = ?",
    )
    .bind(u.id)
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    Ok(Json(
        json!({"ok": true, "data": {"position_ms": pos.unwrap_or(0)}}),
    ))
}

pub async fn fetch_video(st: &AppState, id: i64) -> AppResult<VideoRow> {
    sqlx::query_as::<_, VideoRow>(&format!("SELECT {VIDEO_COLS} FROM videos v WHERE v.id = ?"))
        .bind(id)
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(|| AppError::NotFound("视频不存在".into()))
}

#[allow(dead_code)]
pub type Q = HashMap<String, String>;

const CARD_FIELDS: &str = "v.id, v.title, v.poster_key, v.duration_ms, v.views, \
v.likes_count, v.favorites_count, v.danmaku_count, v.comments_count, v.category, \
v.created_at AS created_at_ms, u.id AS author_id, \
COALESCE(NULLIF(u.display_name, ''), u.username) AS author_name, \
u.avatar_key AS author_avatar_key, '' AS poster_url, '' AS author_avatar, '' AS created_at";

/// 我的收藏
pub async fn my_favorites(
    State(st): State<AppState>,
    user: RequiredUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<crate::util::ApiOk<crate::util::Page<VideoCard>>>> {
    let (limit, offset) = page_bounds(q.page, q.limit);
    let sql = format!(
        "SELECT {} FROM favorites f JOIN videos v ON v.id = f.video_id \
         JOIN users u ON u.id = v.user_id \
         WHERE f.user_id = ? AND v.status = 'ready' \
         ORDER BY f.created_at DESC LIMIT {} OFFSET {}",
        CARD_FIELDS,
        limit + 1,
        offset
    );
    let mut rows = sqlx::query_as::<_, VideoCard>(&sql)
        .bind(user.0.id)
        .fetch_all(&st.db)
        .await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    for r in &mut rows {
        r.fix();
    }
    Ok(ok(crate::util::page(
        rows,
        q.page.unwrap_or(1).max(1),
        limit,
        has_more,
    )))
}

/// 观看历史
pub async fn my_history(
    State(st): State<AppState>,
    user: RequiredUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<crate::util::ApiOk<crate::util::Page<VideoCard>>>> {
    let (limit, offset) = page_bounds(q.page, q.limit);
    let sql = format!(
        "SELECT {} FROM watch_history h JOIN videos v ON v.id = h.video_id \
         JOIN users u ON u.id = v.user_id \
         WHERE h.user_id = ? AND v.status = 'ready' \
         ORDER BY h.updated_at DESC LIMIT {} OFFSET {}",
        CARD_FIELDS,
        limit + 1,
        offset
    );
    let mut rows = sqlx::query_as::<_, VideoCard>(&sql)
        .bind(user.0.id)
        .fetch_all(&st.db)
        .await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    for r in &mut rows {
        r.fix();
    }
    Ok(ok(crate::util::page(
        rows,
        q.page.unwrap_or(1).max(1),
        limit,
        has_more,
    )))
}

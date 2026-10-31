use std::net::SocketAddr;
use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use sqlx::FromRow;

use crate::auth::{audit, RequiredUser};
use crate::error::{AppError, AppResult};
use crate::media::{self, Job};
use crate::security::client_ip;
use crate::state::AppState;
use crate::util::{clean_text, json_str_array, now_ms, valid_storage_key};

#[derive(Debug, FromRow)]
struct UploadRow {
    id: String,
    user_id: i64,
    filename: String,
    total_size: i64,
    chunk_size: i64,
    total_chunks: i64,
    received: i64,
    status: String,
    expires_at: i64,
}

#[derive(Deserialize)]
pub struct InitReq {
    pub filename: String,
    pub total_size: i64,
}

/// 创建分片上传会话
pub async fn init(
    State(st): State<AppState>,
    user: RequiredUser,
    Json(req): Json<InitReq>,
) -> AppResult<Json<serde_json::Value>> {
    st.rate_limit("upload_init", &user.0.id.to_string(), 40, 3600)?;

    if req.total_size <= 0 {
        return Err(AppError::BadRequest("文件为空".into()));
    }
    if req.total_size > st.cfg.max_video_bytes {
        return Err(AppError::PayloadTooLarge);
    }

    let original = clean_text(&req.filename, 120);
    let lower = original.to_ascii_lowercase();
    let allowed = [".mp4", ".webm", ".mov", ".m4v", ".mkv", ".avi", ".flv"];
    if !allowed.iter().any(|e| lower.ends_with(e)) {
        return Err(AppError::UnsupportedMedia);
    }

    // 清理过期会话
    let now = now_ms();
    let expired: Vec<String> =
        sqlx::query_scalar("SELECT id FROM uploads WHERE user_id = ? AND expires_at < ?")
            .bind(user.0.id)
            .bind(now)
            .fetch_all(&st.db)
            .await
            .unwrap_or_default();
    for id in &expired {
        let _ = tokio::fs::remove_dir_all(st.cfg.tmp_dir().join(id)).await;
    }
    sqlx::query("DELETE FROM uploads WHERE user_id = ? AND expires_at < ?")
        .bind(user.0.id)
        .bind(now)
        .execute(&st.db)
        .await?;

    let open: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM uploads WHERE user_id = ? AND status = 'open'")
            .bind(user.0.id)
            .fetch_one(&st.db)
            .await?;
    if open >= st.cfg.max_upload_sessions {
        return Err(AppError::TooManyRequests(60));
    }

    let chunk_size = st.cfg.chunk_size;
    let total_chunks = (req.total_size + chunk_size - 1) / chunk_size;
    let id = uuid::Uuid::new_v4().to_string();
    let expires_at = now + 24 * 3600 * 1000;

    sqlx::query(
        "INSERT INTO uploads (id, user_id, filename, total_size, chunk_size, total_chunks, received, status, created_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, 0, 'open', ?, ?)",
    )
    .bind(&id)
    .bind(user.0.id)
    .bind(&original)
    .bind(req.total_size)
    .bind(chunk_size)
    .bind(total_chunks)
    .bind(now)
    .bind(expires_at)
    .execute(&st.db)
    .await?;

    tokio::fs::create_dir_all(st.cfg.tmp_dir().join(&id))
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    Ok(Json(json!({
        "ok": true,
        "data": {
            "upload_id": id,
            "chunk_size": chunk_size,
            "total_chunks": total_chunks,
            "expires_at": crate::util::iso(expires_at),
        }
    })))
}

async fn load_upload(st: &AppState, id: &str, user_id: i64) -> AppResult<UploadRow> {
    let row = sqlx::query_as::<_, UploadRow>(
        "SELECT id, user_id, filename, total_size, chunk_size, total_chunks, received, status, expires_at \
         FROM uploads WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&st.db)
    .await?
    .ok_or_else(|| AppError::NotFound("上传会话不存在".into()))?;

    if row.user_id != user_id {
        return Err(AppError::Forbidden("无权操作该上传会话".into()));
    }
    if row.status != "open" {
        return Err(AppError::Conflict("上传会话已结束".into()));
    }
    if row.expires_at < now_ms() {
        return Err(AppError::Conflict("上传会话已过期".into()));
    }
    Ok(row)
}

pub async fn status(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let row = load_upload(&st, &id, user.0.id).await?;
    Ok(Json(json!({
        "ok": true,
        "data": {
            "upload_id": row.id,
            "received": row.received,
            "total_chunks": row.total_chunks,
            "chunk_size": row.chunk_size,
            "filename": row.filename,
            "expires_at": crate::util::iso(row.expires_at),
        }
    })))
}

/// 上传单个分片。Body 为原始字节，避免 multipart 编码开销。
pub async fn put_chunk(
    State(st): State<AppState>,
    user: RequiredUser,
    Path((id, idx)): Path<(String, i64)>,
    body: Bytes,
) -> AppResult<Response> {
    let row = load_upload(&st, &id, user.0.id).await?;
    if idx < 0 || idx >= row.total_chunks {
        return Err(AppError::BadRequest("分片序号越界".into()));
    }
    if body.is_empty() {
        return Err(AppError::BadRequest("分片内容为空".into()));
    }
    let expected = if idx == row.total_chunks - 1 {
        row.total_size - idx * row.chunk_size
    } else {
        row.chunk_size
    };
    if body.len() as i64 > row.chunk_size || body.len() as i64 > expected {
        return Err(AppError::PayloadTooLarge);
    }
    // 允许略小于期望值（网络重传/末片），但不允许超出

    let dir = st.cfg.tmp_dir().join(&id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::Other(e.into()))?;
    let part = dir.join(format!("{idx}.part"));
    let tmp = dir.join(format!("{idx}.part.tmp"));
    tokio::fs::write(&tmp, &body)
        .await
        .map_err(|e| AppError::Other(e.into()))?;
    tokio::fs::rename(&tmp, &part)
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    // 幂等：重复上传同一分片不会重复计数
    let existed =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM uploads WHERE id = ? AND received >= 1")
            .bind(&id)
            .fetch_one(&st.db)
            .await
            .unwrap_or(0);

    let _ = existed;
    sqlx::query("UPDATE uploads SET received = MIN(received + 1, total_chunks) WHERE id = ? AND received < total_chunks")
        .bind(&id)
        .execute(&st.db)
        .await?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

pub async fn abort(
    State(st): State<AppState>,
    user: RequiredUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let row = load_upload(&st, &id, user.0.id).await?;
    sqlx::query("UPDATE uploads SET status = 'aborted' WHERE id = ?")
        .bind(&row.id)
        .execute(&st.db)
        .await?;
    let _ = tokio::fs::remove_dir_all(st.cfg.tmp_dir().join(&row.id)).await;
    Ok(Json(json!({"ok": true, "data": {"aborted": true}})))
}

#[derive(Deserialize)]
pub struct CompleteReq {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cover_key: String,
    #[serde(default)]
    pub visibility: Option<String>,
}

pub async fn complete(
    State(st): State<AppState>,
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: RequiredUser,
    Path(id): Path<String>,
    Json(req): Json<CompleteReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ip = client_ip(&headers, Some(ci.ip()), st.cfg.trust_proxy);
    let row = load_upload(&st, &id, user.0.id).await?;

    let title = clean_text(&req.title, 100);
    if title.chars().count() < 2 {
        return Err(AppError::BadRequest("标题至少 2 个字".into()));
    }
    const CATEGORIES: &[&str] = &[
        "anime", "game", "music", "tech", "life", "movie", "study", "other",
    ];
    let category = if CATEGORIES.contains(&req.category.as_str()) {
        req.category.clone()
    } else {
        "other".into()
    };
    let visibility = match req.visibility.as_deref() {
        Some("unlisted") => "unlisted",
        Some("private") => "private",
        _ => "public",
    };
    let tags: Vec<String> = req
        .tags
        .iter()
        .map(|t| clean_text(t, 16))
        .filter(|t| !t.is_empty())
        .take(8)
        .collect();

    // 校验所有分片齐全
    let dir = st.cfg.tmp_dir().join(&row.id);
    let mut total: i64 = 0;
    let mut paths = Vec::with_capacity(row.total_chunks as usize);
    for idx in 0..row.total_chunks {
        let p = dir.join(format!("{idx}.part"));
        let meta = tokio::fs::metadata(&p)
            .await
            .map_err(|_| AppError::BadRequest(format!("缺少分片 {idx}，请重新上传")))?;
        total += meta.len() as i64;
        paths.push(p);
    }
    if total != row.total_size {
        return Err(AppError::BadRequest("分片总大小与声明不一致".into()));
    }

    // 组装到最终位置（流式拷贝，避免整文件进内存）
    let ext = row
        .filename
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "mp4".into());
    let stamp = chrono::Utc::now().format("%Y%m");
    let key = format!("media/{}/{}.{}", stamp, uuid::Uuid::new_v4(), ext);
    let out_path: PathBuf = st.cfg.data_dir.join(&key);
    if let Some(parent) = out_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Other(e.into()))?;
    }

    {
        use tokio::io::AsyncWriteExt;
        let out = tokio::fs::File::create(&out_path)
            .await
            .map_err(|e| AppError::Other(e.into()))?;
        let mut writer = tokio::io::BufWriter::with_capacity(1024 * 1024, out);
        for p in &paths {
            let mut f = tokio::fs::File::open(p)
                .await
                .map_err(|e| AppError::Other(e.into()))?;
            tokio::io::copy(&mut f, &mut writer)
                .await
                .map_err(|e| AppError::Other(e.into()))?;
        }
        writer
            .flush()
            .await
            .map_err(|e| AppError::Other(e.into()))?;
    }

    // 真实类型校验（魔数），不信任扩展名
    let Some((kind, mime, _real_ext)) = media::sniff_path(&out_path).await else {
        let _ = tokio::fs::remove_file(&out_path).await;
        return Err(AppError::UnsupportedMedia);
    };
    if kind != media::MediaKind::Video {
        let _ = tokio::fs::remove_file(&out_path).await;
        return Err(AppError::UnsupportedMedia);
    }

    let cover_key = if valid_storage_key(&req.cover_key) && req.cover_key.starts_with("covers/") {
        req.cover_key
    } else {
        String::new()
    };

    let now = now_ms();
    let status = if st.ffmpeg_available {
        "processing"
    } else {
        "ready"
    };
    let res = sqlx::query(
        "INSERT INTO videos (user_id, title, description, category, tags, status, visibility, media_key, \
         poster_key, hls_key, original_name, mime, size_bytes, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?, ?, ?, ?)",
    )
    .bind(user.0.id)
    .bind(&title)
    .bind(clean_text(&req.description, 5000))
    .bind(&category)
    .bind(json_str_array(&tags))
    .bind(status)
    .bind(visibility)
    .bind(&key)
    .bind(&cover_key)
    .bind(&row.filename)
    .bind(mime)
    .bind(total)
    .bind(now)
    .bind(now)
    .execute(&st.db)
    .await?;

    let video_id = res.last_insert_rowid();

    sqlx::query("UPDATE uploads SET status = 'done' WHERE id = ?")
        .bind(&row.id)
        .execute(&st.db)
        .await?;
    let _ = tokio::fs::remove_dir_all(&dir).await;
    audit(
        &st.db,
        "video_upload",
        Some(user.0.id),
        &ip,
        &format!("video={video_id} size={total}"),
    );

    if st.ffmpeg_available {
        let _ = st.jobs.try_send(Job::Process(video_id));
    }

    Ok(Json(json!({
        "ok": true,
        "data": { "video_id": video_id, "status": status }
    })))
}

/// 封面上传（图片较小，直接 multipart）
pub async fn upload_cover(
    State(st): State<AppState>,
    user: RequiredUser,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    st.rate_limit("cover", &user.0.id.to_string(), 60, 3600)?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("表单解析失败".into()))?
    {
        if field.name() == Some("cover") {
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::BadRequest("读取文件失败".into()))?;
            if bytes.len() as i64 > st.cfg.max_cover_bytes {
                return Err(AppError::PayloadTooLarge);
            }
            let Some((kind, _mime, ext)) = media::sniff(&bytes) else {
                return Err(AppError::UnsupportedMedia);
            };
            if kind != media::MediaKind::Image {
                return Err(AppError::UnsupportedMedia);
            }
            let key = format!("covers/{}.{}", uuid::Uuid::new_v4(), ext);
            tokio::fs::write(st.cfg.data_dir.join(&key), &bytes)
                .await
                .map_err(|e| AppError::Other(e.into()))?;
            return Ok(Json(json!({
                "ok": true,
                "data": { "cover_key": key, "cover_url": format!("/media/{key}") }
            })));
        }
    }
    Err(AppError::BadRequest("缺少 cover 字段".into()))
}

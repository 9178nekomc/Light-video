use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::error::AppResult;
use crate::media::{guess_mime, resolve, serve_file};
use crate::state::AppState;
use crate::util::valid_storage_key;

/// 只允许这三类前缀，避免把 data 目录下的数据库、密钥等敏感文件当静态资源暴露
const PUBLIC_PREFIXES: [&str; 3] = ["media/", "covers/", "avatars/"];

fn public_path(path: &str) -> bool {
    valid_storage_key(path) && PUBLIC_PREFIXES.iter().any(|p| path.starts_with(p))
}

/// /media/{*path} —— 封面、头像、原始视频（支持 Range）
pub async fn media(
    State(st): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if !public_path(&path) {
        return Ok((StatusCode::NOT_FOUND, "not found").into_response());
    }
    let full = resolve(&st.cfg.data_dir, &path)?;
    let mime = guess_mime(&full);

    // 视频原始文件允许 Range + 强缓存（文件名带 UUID，内容不可变）
    let cache = if mime.starts_with("video/") {
        "public, max-age=604800"
    } else {
        "public, max-age=31536000, immutable"
    };
    serve_file(&full, &mime, &headers, cache).await
}

/// /hls/{*path} —— m3u8 短缓存 / ts 强缓存
pub async fn hls(
    State(st): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if !valid_storage_key(&path) {
        return Ok((StatusCode::BAD_REQUEST, "bad path").into_response());
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let full: PathBuf = resolve(&st.cfg.hls_dir(), &path)?;

    let (mime, cache) = match ext.as_str() {
        "m3u8" => ("application/vnd.apple.mpegurl", "public, max-age=5"),
        "ts" => ("video/mp2t", "public, max-age=31536000, immutable"),
        "m4s" | "mp4" => ("video/mp4", "public, max-age=31536000, immutable"),
        _ => ("application/octet-stream", "public, max-age=600"),
    };

    let mut resp = serve_file(&full, mime, &headers, cache).await?;
    // Safari / iOS 需要显式允许跨域媒体访问
    resp.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    Ok(resp)
}

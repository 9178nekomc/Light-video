use std::path::{Path, PathBuf};
use std::process::Stdio;

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::error::{AppError, AppResult};
use crate::util::valid_storage_key;

/// 后台媒体任务
#[derive(Debug, Clone)]
pub enum Job {
    /// 探测时长/分辨率 → 生成 HLS 多码率 → 生成封面
    Process(i64),
}

// ==================== HTTP Range 流式发送 ====================

#[derive(Debug)]
struct RangeSpec {
    start: u64,
    end: u64,
}

fn parse_range(raw: &str, size: u64) -> Option<RangeSpec> {
    let raw = raw.trim();
    let spec = raw.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None; // 多区间请求退化为整文件 200，浏览器可正常播放
    }
    let (a, b) = spec.split_once('-')?;
    let (start, end) = if a.is_empty() {
        // bytes=-N 尾部 N 字节
        let n: u64 = b.trim().parse().ok()?;
        if n == 0 || size == 0 {
            return None;
        }
        (size.saturating_sub(n), size - 1)
    } else {
        let start: u64 = a.trim().parse().ok()?;
        let end: u64 = if b.trim().is_empty() {
            size.saturating_sub(1)
        } else {
            b.trim().parse().ok()?
        };
        (start, end.min(size.saturating_sub(1)))
    };
    if start > end || start >= size {
        return None;
    }
    Some(RangeSpec { start, end })
}

fn etag_for(size: u64, modified: Option<std::time::SystemTime>) -> String {
    let mtime = modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("\"{:x}-{:x}\"", size, mtime)
}

/// 支持 Range / ETag / 缓存的静态媒体响应。
pub async fn serve_file(
    path: &Path,
    mime: &str,
    headers: &HeaderMap,
    cache_control: &'static str,
) -> AppResult<Response> {
    let meta = match tokio::fs::metadata(path).await {
        Ok(m) => m,
        Err(_) => return Err(AppError::NotFound("资源不存在".into())),
    };
    if !meta.is_file() {
        return Err(AppError::NotFound("资源不存在".into()));
    }
    let size = meta.len();
    let etag = etag_for(size, meta.modified().ok());

    // 条件请求
    if let Some(if_none) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if if_none.split(',').any(|t| t.trim() == etag) {
            let mut resp = StatusCode::NOT_MODIFIED.into_response();
            let h = resp.headers_mut();
            h.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
            h.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control),
            );
            return Ok(resp);
        }
    }

    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| parse_range(r, size));

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    let (status, start, len) = match &range {
        Some(r) => (StatusCode::PARTIAL_CONTENT, r.start, r.end - r.start + 1),
        None => (StatusCode::OK, 0, size),
    };

    if start > 0 {
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| AppError::Other(e.into()))?;
    }

    let stream = ReaderStream::with_capacity(file.take(len), 128 * 1024);
    let body = Body::from_stream(stream);

    let mut resp = Response::new(body);
    *resp.status_mut() = status;
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    h.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&len.to_string()).unwrap(),
    );
    if let Some(r) = range {
        h.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {}-{}/{}", r.start, r.end, size)).unwrap(),
        );
    }
    Ok(resp)
}

/// 按存储 key 解析到安全路径（拒绝穿越）
pub fn resolve(base: &Path, key: &str) -> AppResult<PathBuf> {
    if !valid_storage_key(key) {
        return Err(AppError::BadRequest("非法资源路径".into()));
    }
    let p = base.join(key);
    let canonical_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    if let Ok(canon) = p.canonicalize() {
        if !canon.starts_with(&canonical_base) {
            return Err(AppError::Forbidden("越权访问".into()));
        }
    }
    Ok(p)
}

pub fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string()
}

// ==================== 上传文件魔数校验 ====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Image,
}

/// 通过文件头判定真实类型，绝不信任扩展名 / Content-Type
pub fn sniff(head: &[u8]) -> Option<(MediaKind, &'static str, &'static str)> {
    if head.len() < 12 {
        return None;
    }
    // ISO BMFF (mp4 / mov / m4v)
    if &head[4..8] == b"ftyp" {
        let brand = &head[8..12];
        let mime = match brand {
            b"qt  " => "video/quicktime",
            _ => "video/mp4",
        };
        return Some((MediaKind::Video, mime, "mp4"));
    }
    // Matroska / WebM
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let is_webm = head.windows(4).take(64).any(|w| w == b"webm");
        return Some((
            MediaKind::Video,
            if is_webm {
                "video/webm"
            } else {
                "video/x-matroska"
            },
            if is_webm { "webm" } else { "mkv" },
        ));
    }
    // AVI
    if head.starts_with(b"RIFF") && head.len() >= 12 && &head[8..12] == b"AVI " {
        return Some((MediaKind::Video, "video/x-msvideo", "avi"));
    }
    // FLV
    if head.starts_with(b"FLV\x01") {
        return Some((MediaKind::Video, "video/x-flv", "flv"));
    }
    // MPEG-TS（0x47 同步字节）
    if head[0] == 0x47 && head[188] == 0x47 {
        return Some((MediaKind::Video, "video/mp2t", "ts"));
    }
    // 图片
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some((MediaKind::Image, "image/jpeg", "jpg"));
    }
    if head.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some((MediaKind::Image, "image/png", "png"));
    }
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        return Some((MediaKind::Image, "image/gif", "gif"));
    }
    if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return Some((MediaKind::Image, "image/webp", "webp"));
    }
    None
}

pub async fn sniff_path(path: &Path) -> Option<(MediaKind, &'static str, &'static str)> {
    let mut f = tokio::fs::File::open(path).await.ok()?;
    let mut head = vec![0u8; 512];
    let n = f.read(&mut head).await.ok()?;
    sniff(&head[..n])
}

// ==================== 外部工具（ffmpeg / ffprobe） ====================

pub async fn binary_available(bin: &str) -> bool {
    tokio::process::Command::new(bin)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Default)]
pub struct ProbeInfo {
    pub duration_ms: i64,
    pub width: i64,
    pub height: i64,
}

pub async fn probe(ffprobe: &str, input: &Path) -> Option<ProbeInfo> {
    let out = tokio::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,duration:format=duration",
            "-of",
            "json",
        ])
        .arg(input)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let stream = v.get("streams")?.get(0)?;
    let width = stream.get("width").and_then(|x| x.as_i64()).unwrap_or(0);
    let height = stream.get("height").and_then(|x| x.as_i64()).unwrap_or(0);
    let duration = stream
        .get("duration")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            v.get("format")
                .and_then(|f| f.get("duration"))
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    Some(ProbeInfo {
        duration_ms: (duration * 1000.0) as i64,
        width,
        height,
    })
}

pub struct LadderStep {
    pub height: i64,
    pub maxrate: i64,
    pub bufsize: i64,
    pub audio_kbps: i64,
}

/// 依据源分辨率生成码率阶梯（不会上采样）
pub fn ladder(src_height: i64) -> Vec<LadderStep> {
    let all = [
        LadderStep {
            height: 360,
            maxrate: 850,
            bufsize: 1400,
            audio_kbps: 96,
        },
        LadderStep {
            height: 540,
            maxrate: 1600,
            bufsize: 2600,
            audio_kbps: 112,
        },
        LadderStep {
            height: 720,
            maxrate: 2800,
            bufsize: 4500,
            audio_kbps: 128,
        },
        LadderStep {
            height: 1080,
            maxrate: 5200,
            bufsize: 8400,
            audio_kbps: 160,
        },
    ];
    let cap = src_height.clamp(240, 1080);
    let mut out: Vec<LadderStep> = all.into_iter().filter(|s| s.height <= cap).collect();
    if out.is_empty() {
        out.push(LadderStep {
            height: cap,
            maxrate: 700,
            bufsize: 1200,
            audio_kbps: 96,
        });
    }
    out
}

pub fn rendition_name(height: i64) -> String {
    format!("{}p", height)
}

/// 生成单档 MP4（faststart）。MP4 渐进式 + Range 请求在所有浏览器都能秒开、拖动，
/// 切换清晰度只需换 src 并恢复播放位置，兼容性远好于需要 MSE 的 HLS。
pub async fn transcode_mp4(
    ffmpeg: &str,
    input: &Path,
    out: &Path,
    step: &LadderStep,
    src_height: i64,
) -> anyhow::Result<()> {
    if let Some(parent) = out.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // 源比目标小则等比缩小，-2 保证宽为偶数（libx264 要求）
    let target = step.height.min(src_height);
    let vf = format!("scale=-2:{target}");

    let status = tokio::process::Command::new(ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(input)
        .args([
            "-vf",
            &vf,
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-profile:v",
            "main",
            "-crf",
            "21",
            "-maxrate",
            &format!("{}k", step.maxrate),
            "-bufsize",
            &format!("{}k", step.bufsize),
            "-g",
            "48",
            "-keyint_min",
            "48",
            "-sc_threshold",
            "0",
            "-c:a",
            "aac",
            "-b:a",
            &format!("{}k", step.audio_kbps),
            "-ac",
            "2",
            "-movflags",
            "+faststart",
        ])
        .arg(out)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .await?;

    if !status.success() {
        // 失败时清理半成品，避免被当成可用清晰度
        let _ = tokio::fs::remove_file(out).await;
        anyhow::bail!("ffmpeg 转码失败: {}", rendition_name(target));
    }
    Ok(())
}

pub async fn make_poster(ffmpeg: &str, input: &Path, out: &Path) -> anyhow::Result<()> {
    if let Some(parent) = out.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let status = tokio::process::Command::new(ffmpeg)
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            "00:00:02",
            "-i",
        ])
        .arg(input)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale='min(1280,iw)':-2",
            "-q:v",
            "3",
        ])
        .arg(out)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    if !status.success() {
        anyhow::bail!("封面生成失败");
    }
    Ok(())
}

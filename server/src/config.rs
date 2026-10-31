use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rand::RngCore;

/// 全局配置。全部来自环境变量，带安全默认值。
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// 对外访问地址（用于 OAuth 回调、绝对 URL）
    pub public_url: String,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub jwt_secret: Arc<Vec<u8>>,
    pub session_days: i64,
    /// 生产环境必须为 true（HTTPS）
    pub secure_cookies: bool,
    /// 允许的来源，用于 WS Origin 校验与 CORS；空表示仅同源
    pub allowed_origins: Vec<String>,
    pub trust_proxy: bool,
    pub max_video_bytes: i64,
    pub max_cover_bytes: i64,
    pub chunk_size: i64,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    /// 单用户并发上传会话上限
    pub max_upload_sessions: i64,
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

/// 读取或生成持久化的 JWT 密钥，权限 600。
fn load_secret(data_dir: &Path) -> Arc<Vec<u8>> {
    if let Ok(s) = std::env::var("JWT_SECRET") {
        if s.len() >= 32 {
            return Arc::new(s.into_bytes());
        }
        tracing::warn!("JWT_SECRET 长度不足 32 字节，已忽略并使用自动生成的密钥");
    }
    let path = data_dir.join("jwt.key");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() >= 32 {
            return Arc::new(bytes);
        }
    }
    let mut key = vec![0u8; 64];
    rand::thread_rng().fill_bytes(&mut key);
    let hex_key = hex::encode(&key);
    if let Err(e) = std::fs::write(&path, hex_key.as_bytes()) {
        tracing::error!("无法写入 JWT 密钥文件: {e}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Arc::new(hex_key.into_bytes())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();

        let data_dir = PathBuf::from(env_str("DATA_DIR", "./data"));
        std::fs::create_dir_all(&data_dir)?;
        for sub in ["media", "hls", "tmp", "covers", "avatars"] {
            std::fs::create_dir_all(data_dir.join(sub))?;
        }

        let db_path = PathBuf::from(env_str(
            "DATABASE_PATH",
            data_dir
                .join("lightvideo.db")
                .to_str()
                .unwrap_or("./data/lightvideo.db"),
        ));

        let host = env_str("BIND_HOST", "0.0.0.0");
        let port: u16 = env_parse("PORT", 3000);
        let bind: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|_| anyhow::anyhow!("无效的监听地址 {host}:{port}"))?;

        let public_url = env_str("PUBLIC_URL", &format!("http://localhost:{port}"));
        let allowed_origins = env_str("ALLOWED_ORIGINS", "")
            .split(',')
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let mut cfg = Self {
            bind,
            public_url: public_url.trim_end_matches('/').to_string(),
            jwt_secret: Arc::new(Vec::new()),
            session_days: env_parse("SESSION_DAYS", 14),
            secure_cookies: env_bool("SECURE_COOKIES", false),
            allowed_origins,
            trust_proxy: env_bool("TRUST_PROXY", false),
            max_video_bytes: env_parse("MAX_VIDEO_MB", 4096) * 1024 * 1024,
            max_cover_bytes: env_parse("MAX_COVER_MB", 12) * 1024 * 1024,
            chunk_size: env_parse("CHUNK_SIZE_MB", 4) * 1024 * 1024,
            ffmpeg_path: env_str("FFMPEG_PATH", "ffmpeg"),
            ffprobe_path: env_str("FFPROBE_PATH", "ffprobe"),
            max_upload_sessions: env_parse("MAX_UPLOAD_SESSIONS", 6),
            db_path: db_path.clone(),
            data_dir: data_dir.clone(),
        };
        cfg.jwt_secret = load_secret(&data_dir);
        Ok(cfg)
    }

    pub fn media_dir(&self) -> PathBuf {
        self.data_dir.join("media")
    }
    pub fn hls_dir(&self) -> PathBuf {
        self.data_dir.join("hls")
    }
    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }
    #[allow(dead_code)]
    pub fn covers_dir(&self) -> PathBuf {
        self.data_dir.join("covers")
    }
    #[allow(dead_code)]
    pub fn avatars_dir(&self) -> PathBuf {
        self.data_dir.join("avatars")
    }
}

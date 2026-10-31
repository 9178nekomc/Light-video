use std::sync::Arc;

use dashmap::DashMap;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::media::Job;
use crate::security::RateLimiter;
use crate::ws::Hub;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub db: SqlitePool,
    pub hub: Arc<Hub>,
    pub limiter: Arc<RateLimiter>,
    pub sessions: Arc<crate::auth::SessionCache>,
    pub views: Arc<ViewTracker>,
    pub jobs: mpsc::Sender<Job>,
    pub ffmpeg_available: bool,
    pub started_at: i64,
}

/// 播放量统计：内存聚合 + 定时批量落盘，避免每次播放都写库
#[derive(Default)]
pub struct ViewTracker {
    pending: DashMap<i64, i64>,
    seen: DashMap<String, std::time::Instant>,
}

impl ViewTracker {
    pub fn hit(&self, video_id: i64, client_key: &str, window_secs: u64) -> bool {
        let key = format!("{video_id}:{client_key}");
        let now = std::time::Instant::now();
        if let Some(prev) = self.seen.get(&key) {
            if now.duration_since(*prev).as_secs() < window_secs {
                return false;
            }
        }
        self.seen.insert(key, now);
        *self.pending.entry(video_id).or_insert(0) += 1;
        true
    }

    /// 取出并清空待写入的增量
    pub fn drain(&self) -> Vec<(i64, i64)> {
        let mut out = Vec::new();
        let keys: Vec<i64> = self.pending.iter().map(|e| *e.key()).collect();
        for k in keys {
            if let Some((_, v)) = self.pending.remove(&k) {
                if v > 0 {
                    out.push((k, v));
                }
            }
        }
        out
    }

    pub fn gc_seen(&self) {
        let now = std::time::Instant::now();
        self.seen
            .retain(|_, t| now.duration_since(*t).as_secs() < 6 * 3600);
    }
}

impl AppState {
    pub fn new(
        cfg: Arc<Config>,
        db: SqlitePool,
        jobs: mpsc::Sender<Job>,
        ffmpeg_available: bool,
    ) -> Self {
        Self {
            cfg,
            db,
            hub: Hub::new(),
            limiter: RateLimiter::new(),
            sessions: Arc::new(crate::auth::SessionCache::new()),
            views: Arc::new(ViewTracker::default()),
            jobs,
            ffmpeg_available,
            started_at: crate::util::now_ms(),
        }
    }

    /// 统一限流入口：key = scope + 客户端标识
    pub fn rate_limit(
        &self,
        scope: &str,
        identity: &str,
        max: u32,
        window_secs: u64,
    ) -> Result<(), crate::error::AppError> {
        self.limiter
            .check(
                &format!("{scope}:{identity}"),
                crate::security::Limit::new(max, window_secs),
            )
            .map_err(crate::error::AppError::TooManyRequests)
    }
}

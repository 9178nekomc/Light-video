//! Light-Video 服务进程入口：运行时、后台任务、优雅退出。
//! 业务逻辑全部在 `lightvideo` 库中，便于集成测试直接驱动 Router。

use std::sync::Arc;
use std::time::Duration;

use lightvideo::config::Config;
use lightvideo::media::{self, Job};
use lightvideo::state::AppState;
use lightvideo::util;

use tokio::sync::mpsc;

fn main() -> anyhow::Result<()> {
    // 工作线程：默认占满所有核心；WORKER_THREADS 可覆盖
    let threads = std::env::var("WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(num_cpus);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .thread_name("lv-worker")
        .build()?;

    rt.block_on(async_main())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

async fn async_main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lightvideo=info,tower_http=warn".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let cfg = Arc::new(Config::from_env()?);

    // SQLite：WAL 下读并发优秀、写串行；连接数取 2×CPU ∩ [8,32]
    let db_conns = std::cmp::max(8, (num_cpus() * 2).min(32)) as u32;
    let pool = lightvideo::db::connect(&cfg.db_path, db_conns).await?;
    tracing::info!(
        "数据库就绪: {} (连接池 {} 条)",
        cfg.db_path.display(),
        db_conns
    );

    let (job_tx, job_rx) = mpsc::channel::<Job>(128);
    let ffmpeg_available = media::binary_available(&cfg.ffmpeg_path).await;
    if ffmpeg_available {
        tracing::info!("检测到 ffmpeg：上传后将自动生成多分辨率版本");
    } else {
        tracing::warn!(
            "未检测到 ffmpeg：使用渐进式 MP4 + Range 播放（转码自动降级，功能不受影响）"
        );
    }

    let state = AppState::new(cfg.clone(), pool, job_tx, ffmpeg_available);
    spawn_workers(state.clone(), job_rx);
    spawn_maintenance(state.clone());

    let web_dir = resolve_web_dir();
    tracing::info!("静态资源目录: {}", web_dir.display());

    let app = lightvideo::app(state.clone(), web_dir);

    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    tracing::info!("Light-Video 已启动 → http://{}", cfg.bind);
    tracing::info!("对外地址: {}", cfg.public_url);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    flush_views(&state).await;
    tracing::info!("已优雅退出");
    Ok(())
}

fn resolve_web_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("WEB_DIR") {
        return std::path::PathBuf::from(dir);
    }
    for c in ["web", "../web", "./public", "../public"] {
        let p = std::path::PathBuf::from(c);
        if p.join("index.html").exists() {
            return p;
        }
    }
    std::path::PathBuf::from("web")
}

fn spawn_workers(state: AppState, mut rx: mpsc::Receiver<Job>) {
    tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            match job {
                Job::Process(video_id) => {
                    if let Err(e) = process_video(&state, video_id).await {
                        tracing::error!("视频 {video_id} 处理失败: {e:#}");
                        // 转码失败不阻塞播放：保留原片直出
                        let _ = sqlx::query("UPDATE videos SET status = 'ready' WHERE id = ?")
                            .bind(video_id)
                            .execute(&state.db)
                            .await;
                    }
                }
            }
        }
    });
}

/// 视频处理流水线：探测 → 抽帧封面 → 多分辨率 MP4
async fn process_video(st: &AppState, video_id: i64) -> anyhow::Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT media_key, poster_key FROM videos WHERE id = ?")
            .bind(video_id)
            .fetch_optional(&st.db)
            .await?;
    let Some((media_key, poster_key)) = row else {
        return Ok(());
    };

    let input = st.cfg.data_dir.join(&media_key);
    if !input.exists() {
        anyhow::bail!("源文件不存在: {}", input.display());
    }

    let mut width = 0i64;
    let mut height = 0i64;
    let mut duration_ms = 0i64;

    if let Some(info) = media::probe(&st.cfg.ffprobe_path, &input).await {
        width = info.width;
        height = info.height;
        duration_ms = info.duration_ms;
        sqlx::query("UPDATE videos SET width = ?, height = ?, duration_ms = ? WHERE id = ?")
            .bind(width)
            .bind(height)
            .bind(duration_ms)
            .bind(video_id)
            .execute(&st.db)
            .await?;
    }

    if poster_key.is_empty() && height > 0 {
        let key = format!("covers/{}.jpg", video_id);
        let out = st.cfg.data_dir.join(&key);
        if media::make_poster(&st.cfg.ffmpeg_path, &input, &out)
            .await
            .is_ok()
        {
            sqlx::query("UPDATE videos SET poster_key = ? WHERE id = ?")
                .bind(&key)
                .bind(video_id)
                .execute(&st.db)
                .await?;
        }
    }

    // 多分辨率 MP4（原片保留，作为「原画」）
    if height > 0 {
        let base = st.cfg.media_dir().join(video_id.to_string());
        tokio::fs::create_dir_all(&base).await?;
        let aspect = if height > 0 {
            width as f64 / height as f64
        } else {
            16.0 / 9.0
        };

        for step in media::ladder(height) {
            let target_h = step.height.min(height);
            let name = media::rendition_name(target_h);
            let key = format!("media/{video_id}/{name}.mp4");
            let out = st.cfg.data_dir.join(&key);
            match media::transcode_mp4(&st.cfg.ffmpeg_path, &input, &out, &step, height).await {
                Ok(()) => {
                    let target_w = ((target_h as f64 * aspect).round() as i64 / 2) * 2;
                    let _ = sqlx::query(
                        "INSERT OR REPLACE INTO video_renditions (video_id, name, width, height, bitrate_kbps, playlist_key) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                    )
                    .bind(video_id)
                    .bind(&name)
                    .bind(target_w)
                    .bind(target_h)
                    .bind(step.maxrate)
                    .bind(&key)
                    .execute(&st.db)
                    .await;
                    tracing::info!(video_id, "{name} 转码完成");
                }
                Err(e) => tracing::warn!("{target_h}p 转码失败: {e:#}"),
            }
        }
    }

    sqlx::query("UPDATE videos SET status = 'ready', updated_at = ? WHERE id = ?")
        .bind(util::now_ms())
        .bind(video_id)
        .execute(&st.db)
        .await?;

    tracing::info!(video_id, "处理完成 {width}x{height} {duration_ms}ms");
    Ok(())
}

fn spawn_maintenance(state: AppState) {
    let st = state.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            flush_views(&st).await;
        }
    });

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        let mut round: u64 = 0;
        loop {
            ticker.tick().await;
            round += 1;
            state.limiter.gc();
            state.sessions.gc();
            if round % 10 == 0 {
                state.views.gc_seen();
            }
            if round % 30 == 0 {
                cleanup_expired_uploads(&state).await;
            }
        }
    });
}

async fn flush_views(st: &AppState) {
    let pending = st.views.drain();
    if pending.is_empty() {
        return;
    }
    for (video_id, delta) in pending {
        let _ = sqlx::query("UPDATE videos SET views = views + ? WHERE id = ?")
            .bind(delta)
            .bind(video_id)
            .execute(&st.db)
            .await;
    }
}

async fn cleanup_expired_uploads(st: &AppState) {
    let cutoff = util::now_ms() - 24 * 3600 * 1000;
    if let Ok(rows) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM uploads WHERE status = 'open' AND expires_at < ?",
    )
    .bind(cutoff)
    .fetch_all(&st.db)
    .await
    {
        for id in rows {
            let _ = tokio::fs::remove_dir_all(st.cfg.tmp_dir().join(&id)).await;
        }
    }
    let _ = sqlx::query("DELETE FROM uploads WHERE status != 'open' AND created_at < ?")
        .bind(util::now_ms() - 7 * 24 * 3600 * 1000)
        .execute(&st.db)
        .await;
    let _ = sqlx::query("DELETE FROM audit_log WHERE created_at < ?")
        .bind(util::now_ms() - 30i64 * 24 * 3600 * 1000)
        .execute(&st.db)
        .await;

    if let Ok(mut dir) = tokio::fs::read_dir(st.cfg.tmp_dir()).await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                if let Ok(modified) = meta.modified() {
                    if modified.elapsed().unwrap_or_default() > Duration::from_secs(86400) {
                        let _ = tokio::fs::remove_dir_all(entry.path()).await;
                    }
                }
            }
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("收到退出信号，正在关闭…");
}

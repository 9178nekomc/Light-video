use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

/// 建立针对高并发读、单写优化的 SQLite 连接池。
///
/// 关键点：
/// - WAL：读写不互斥，读并发不受写阻塞
/// - synchronous=NORMAL：WAL 下仍保证崩溃一致性，写入吞吐提升数倍
/// - busy_timeout：写锁竞争时排队而不是立刻报 SQLITE_BUSY
/// - mmap_size / cache_size：减少页缓存系统调用
/// - temp_store=MEMORY：排序/临时表走内存
pub async fn connect(path: &Path, max_conns: u32) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true)
        .pragma("cache_size", "-65536") // 64 MiB page cache
        .pragma("mmap_size", "268435456")
        .pragma("temp_store", "MEMORY")
        .pragma("wal_autocheckpoint", "2000");

    let pool = SqlitePoolOptions::new()
        .max_connections(max_conns)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(600))
        .test_before_acquire(false)
        .connect_with(opts)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

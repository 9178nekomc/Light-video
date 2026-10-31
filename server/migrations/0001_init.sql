-- Light-Video 数据模型 v1
-- 说明：所有时间戳统一为 Unix 毫秒（INTEGER），便于索引、排序与跨时区一致

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    username       TEXT    NOT NULL COLLATE NOCASE UNIQUE,
    email          TEXT    NOT NULL COLLATE NOCASE UNIQUE,
    password_hash  TEXT    NOT NULL,
    display_name   TEXT    NOT NULL DEFAULT '',
    avatar_key     TEXT    NOT NULL DEFAULT '',
    bio            TEXT    NOT NULL DEFAULT '',
    role           TEXT    NOT NULL DEFAULT 'user',
    token_version  INTEGER NOT NULL DEFAULT 1,
    is_banned      INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_users_name ON users(username);

CREATE TABLE IF NOT EXISTS videos (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id        INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title          TEXT    NOT NULL,
    description    TEXT    NOT NULL DEFAULT '',
    category       TEXT    NOT NULL DEFAULT 'other',
    tags           TEXT    NOT NULL DEFAULT '[]',
    status         TEXT    NOT NULL DEFAULT 'processing', -- processing | ready | failed
    visibility     TEXT    NOT NULL DEFAULT 'public',     -- public | unlisted | private
    media_key      TEXT    NOT NULL DEFAULT '',
    poster_key     TEXT    NOT NULL DEFAULT '',
    hls_key        TEXT    NOT NULL DEFAULT '',
    original_name  TEXT    NOT NULL DEFAULT '',
    mime           TEXT    NOT NULL DEFAULT 'video/mp4',
    size_bytes     INTEGER NOT NULL DEFAULT 0,
    duration_ms    INTEGER NOT NULL DEFAULT 0,
    width          INTEGER NOT NULL DEFAULT 0,
    height         INTEGER NOT NULL DEFAULT 0,
    views          INTEGER NOT NULL DEFAULT 0,
    likes_count    INTEGER NOT NULL DEFAULT 0,
    favorites_count INTEGER NOT NULL DEFAULT 0,
    comments_count INTEGER NOT NULL DEFAULT 0,
    danmaku_count  INTEGER NOT NULL DEFAULT 0,
    shares_count   INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_videos_feed   ON videos(status, visibility, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_videos_user   ON videos(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_videos_hot    ON videos(status, visibility, views DESC);
CREATE INDEX IF NOT EXISTS idx_videos_cat    ON videos(category, created_at DESC);

CREATE TABLE IF NOT EXISTS video_renditions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    video_id     INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,     -- 360p / 720p / 1080p
    width        INTEGER NOT NULL,
    height       INTEGER NOT NULL,
    bitrate_kbps INTEGER NOT NULL DEFAULT 0,
    playlist_key TEXT    NOT NULL DEFAULT '',
    UNIQUE(video_id, name)
);

CREATE TABLE IF NOT EXISTS uploads (
    id           TEXT    PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    filename     TEXT    NOT NULL,
    total_size   INTEGER NOT NULL,
    chunk_size   INTEGER NOT NULL,
    total_chunks INTEGER NOT NULL,
    received     INTEGER NOT NULL DEFAULT 0,
    status       TEXT    NOT NULL DEFAULT 'open',  -- open | done | aborted
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_uploads_user ON uploads(user_id, status);

CREATE TABLE IF NOT EXISTS likes (
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    video_id   INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, video_id)
);
CREATE INDEX IF NOT EXISTS idx_likes_video ON likes(video_id);

CREATE TABLE IF NOT EXISTS favorites (
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    video_id   INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, video_id)
);
CREATE INDEX IF NOT EXISTS idx_fav_video ON favorites(video_id);

CREATE TABLE IF NOT EXISTS comments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    video_id   INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_id  INTEGER REFERENCES comments(id) ON DELETE CASCADE,
    content    TEXT    NOT NULL,
    likes_count INTEGER NOT NULL DEFAULT 0,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_comments_video ON comments(video_id, id DESC);

CREATE TABLE IF NOT EXISTS danmaku (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    video_id   INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    user_id    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    content    TEXT    NOT NULL,
    time_ms    INTEGER NOT NULL,
    color      TEXT    NOT NULL DEFAULT '#ffffff',
    mode       TEXT    NOT NULL DEFAULT 'scroll', -- scroll | top | bottom
    font_size  INTEGER NOT NULL DEFAULT 25,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_danmaku_video ON danmaku(video_id, time_ms);

CREATE TABLE IF NOT EXISTS friendships (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    requester_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    addressee_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status       TEXT    NOT NULL DEFAULT 'pending', -- pending | accepted | rejected
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    UNIQUE(requester_id, addressee_id)
);
CREATE INDEX IF NOT EXISTS idx_friend_req ON friendships(addressee_id, status);
CREATE INDEX IF NOT EXISTS idx_friend_src ON friendships(requester_id, status);

CREATE TABLE IF NOT EXISTS messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    sender_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    receiver_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content     TEXT    NOT NULL,
    kind        TEXT    NOT NULL DEFAULT 'text', -- text | video | system
    video_id    INTEGER REFERENCES videos(id) ON DELETE SET NULL,
    is_read     INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_msg_pair ON messages(sender_id, receiver_id, id DESC);
CREATE INDEX IF NOT EXISTS idx_msg_unread ON messages(receiver_id, is_read);

CREATE TABLE IF NOT EXISTS notifications (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL,
    actor_id   INTEGER REFERENCES users(id) ON DELETE SET NULL,
    video_id   INTEGER REFERENCES videos(id) ON DELETE CASCADE,
    comment_id INTEGER REFERENCES comments(id) ON DELETE CASCADE,
    content    TEXT    NOT NULL,
    is_read    INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notif_user ON notifications(user_id, is_read, id DESC);

CREATE TABLE IF NOT EXISTS watch_history (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    video_id    INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    position_ms INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (user_id, video_id)
);
CREATE INDEX IF NOT EXISTS idx_history_user ON watch_history(user_id, updated_at DESC);

-- 安全审计（登录失败、越权尝试等），仅保留最近记录
CREATE TABLE IF NOT EXISTS audit_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    action     TEXT    NOT NULL,
    user_id    INTEGER,
    ip         TEXT    NOT NULL DEFAULT '',
    detail     TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_time ON audit_log(created_at DESC);

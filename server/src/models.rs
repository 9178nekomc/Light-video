use serde::Serialize;
use sqlx::FromRow;

use crate::util::iso;

pub const USER_COLS: &str = "id, username, email, password_hash, display_name, avatar_key, bio, \
role, token_version, is_banned, created_at";

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub email: String,
    #[allow(dead_code)]
    pub password_hash: String,
    pub display_name: String,
    pub avatar_key: String,
    pub bio: String,
    pub role: String,
    pub token_version: i64,
    pub is_banned: i64,
    pub created_at: i64,
}

impl UserRow {
    pub fn avatar_url(&self) -> String {
        if self.avatar_key.is_empty() {
            String::new()
        } else {
            format!("/media/{}", self.avatar_key)
        }
    }

    pub fn public(&self) -> UserPublic {
        UserPublic {
            id: self.id,
            username: self.username.clone(),
            display_name: if self.display_name.is_empty() {
                self.username.clone()
            } else {
                self.display_name.clone()
            },
            avatar_url: self.avatar_url(),
            bio: self.bio.clone(),
            created_at: iso(self.created_at),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UserPublic {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub bio: String,
    pub created_at: String,
}

/// 登录态用户（请求扩展）
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub token_version: i64,
    #[allow(dead_code)]
    pub is_banned: bool,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct VideoRow {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: String,
    pub category: String,
    pub tags: String,
    pub status: String,
    pub visibility: String,
    pub media_key: String,
    pub poster_key: String,
    pub hls_key: String,
    #[allow(dead_code)]
    pub original_name: String,
    pub mime: String,
    pub size_bytes: i64,
    pub duration_ms: i64,
    pub width: i64,
    pub height: i64,
    pub views: i64,
    pub likes_count: i64,
    pub favorites_count: i64,
    pub comments_count: i64,
    pub danmaku_count: i64,
    #[allow(dead_code)]
    pub shares_count: i64,
    pub created_at: i64,
    #[allow(dead_code)]
    pub updated_at: i64,
}

pub const VIDEO_COLS: &str = "v.id, v.user_id, v.title, v.description, v.category, v.tags, \
v.status, v.visibility, v.media_key, v.poster_key, v.hls_key, v.original_name, v.mime, \
v.size_bytes, v.duration_ms, v.width, v.height, v.views, v.likes_count, v.favorites_count, \
v.comments_count, v.danmaku_count, v.shares_count, v.created_at, v.updated_at";

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct VideoCard {
    pub id: i64,
    pub title: String,
    #[serde(skip)]
    pub poster_key: String,
    pub duration_ms: i64,
    pub views: i64,
    pub likes_count: i64,
    pub favorites_count: i64,
    pub danmaku_count: i64,
    pub comments_count: i64,
    pub category: String,
    #[serde(skip)]
    pub created_at_ms: i64,
    pub author_id: i64,
    pub author_name: String,
    #[serde(skip)]
    pub author_avatar_key: String,
    pub poster_url: String,
    pub author_avatar: String,
    pub created_at: String,
}

impl VideoCard {
    pub fn fix(&mut self) {
        self.poster_url = if self.poster_key.is_empty() {
            String::new()
        } else {
            format!("/media/{}", self.poster_key)
        };
        self.author_avatar = if self.author_avatar_key.is_empty() {
            String::new()
        } else {
            format!("/media/{}", self.author_avatar_key)
        };
        self.created_at = iso(self.created_at_ms);
        if self.author_name.is_empty() {
            self.author_name = "已注销用户".into();
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RenditionRow {
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub bitrate_kbps: i64,
    #[serde(skip)]
    pub playlist_key: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoDetail {
    #[serde(flatten)]
    pub card: VideoCard,
    pub description: String,
    pub tags: Vec<String>,
    pub status: String,
    pub visibility: String,
    pub width: i64,
    pub height: i64,
    pub size_bytes: i64,
    pub mime: String,
    pub stream_url: String,
    pub hls_url: String,
    pub renditions: Vec<RenditionRow>,
    pub liked: bool,
    pub favorited: bool,
    pub following: bool,
    pub is_owner: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DanmakuRow {
    pub id: i64,
    pub content: String,
    pub time_ms: i64,
    pub color: String,
    pub mode: String,
    pub font_size: i64,
    pub user_id: Option<i64>,
    pub username: String,
    #[serde(skip)]
    pub created_at_ms: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CommentRow {
    pub id: i64,
    pub content: String,
    pub parent_id: Option<i64>,
    pub likes_count: i64,
    pub user_id: i64,
    pub username: String,
    #[serde(skip)]
    pub avatar_key: String,
    pub avatar_url: String,
    #[serde(skip)]
    pub created_at_ms: i64,
    pub created_at: String,
    pub is_owner: bool,
}

impl CommentRow {
    pub fn fix(&mut self, viewer: Option<i64>) {
        self.avatar_url = if self.avatar_key.is_empty() {
            String::new()
        } else {
            format!("/media/{}", self.avatar_key)
        };
        self.created_at = iso(self.created_at_ms);
        self.is_owner = viewer == Some(self.user_id);
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MessageRow {
    pub id: i64,
    pub sender_id: i64,
    pub receiver_id: i64,
    pub content: String,
    pub kind: String,
    pub video_id: Option<i64>,
    pub is_read: i64,
    #[serde(skip)]
    pub created_at_ms: i64,
    pub created_at: String,
}

impl MessageRow {
    pub fn fix(&mut self) {
        self.created_at = iso(self.created_at_ms);
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct NotificationRow {
    pub id: i64,
    pub kind: String,
    pub content: String,
    pub video_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub is_read: i64,
    #[serde(skip)]
    pub created_at_ms: i64,
    pub created_at: String,
    pub actor_name: Option<String>,
}

impl NotificationRow {
    pub fn fix(&mut self) {
        self.created_at = iso(self.created_at_ms);
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ConversationRow {
    pub user_id: i64,
    pub username: String,
    pub avatar_key: String,
    pub last_content: String,
    pub last_at: i64,
    pub unread: i64,
    pub avatar_url: String,
    pub last_time: String,
}

impl ConversationRow {
    pub fn fix(&mut self) {
        self.avatar_url = if self.avatar_key.is_empty() {
            String::new()
        } else {
            format!("/media/{}", self.avatar_key)
        };
        self.last_time = iso(self.last_at);
    }
}

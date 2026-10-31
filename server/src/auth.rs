use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use dashmap::DashMap;

use crate::error::AppError;
use crate::models::{AuthUser, UserRow};
use crate::security::{decode_token, parse_cookies, SESSION_COOKIE};
use crate::state::AppState;

const CACHE_TTL: Duration = Duration::from_secs(45);

/// 会话缓存：JWT 校验免签名验证后的数据库回查，兼顾「立即吊销」与吞吐。
pub struct SessionCache {
    map: DashMap<i64, (AuthUser, Instant)>,
}

impl SessionCache {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }
    pub fn get(&self, id: i64) -> Option<AuthUser> {
        if let Some(e) = self.map.get(&id) {
            if e.1.elapsed() < CACHE_TTL {
                return Some(e.0.clone());
            }
        }
        None
    }
    pub fn put(&self, user: AuthUser) {
        self.map.insert(user.id, (user, Instant::now()));
    }
    pub fn invalidate(&self, id: i64) {
        self.map.remove(&id);
    }
    pub fn gc(&self) {
        let now = Instant::now();
        self.map
            .retain(|_, v| now.duration_since(v.1) < CACHE_TTL * 4);
    }
}

impl Default for SessionCache {
    fn default() -> Self {
        Self::new()
    }
}

/// 从请求头解析登录态（HTTP 与 WebSocket 共用）
pub async fn resolve_session(state: &AppState, headers: &HeaderMap) -> Option<AuthUser> {
    let cookies = parse_cookies(headers);
    let token = cookies.get(SESSION_COOKIE)?;
    let claims = decode_token(&state.cfg.jwt_secret, token)?;
    let uid: i64 = claims.sub.parse().ok()?;

    if let Some(cached) = state.sessions.get(uid) {
        if cached.token_version == claims.ver {
            return Some(cached);
        }
        state.sessions.invalidate(uid);
        return None;
    }

    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, email, password_hash, display_name, avatar_key, bio, role, \
         token_version, is_banned, created_at FROM users WHERE id = ?",
    )
    .bind(uid)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()?;

    if row.token_version != claims.ver || row.is_banned != 0 {
        return None;
    }

    let user = AuthUser {
        id: row.id,
        username: row.username,
        role: row.role,
        token_version: row.token_version,
        is_banned: row.is_banned != 0,
    };
    state.sessions.put(user.clone());
    Some(user)
}

/// 必须登录
pub struct RequiredUser(pub AuthUser);

impl FromRequestParts<AppState> for RequiredUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 只允许 cookie 鉴权，杜绝 Bearer token 带来的 XSS 窃取面
        match resolve_session(state, &parts.headers).await {
            Some(u) => Ok(RequiredUser(u)),
            None => Err(AppError::Unauthorized),
        }
    }
}

/// 可选登录态
pub struct MaybeUser(pub Option<AuthUser>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeUser(resolve_session(state, &parts.headers).await))
    }
}

/// 写入审计日志（失败不阻塞业务）
pub fn audit(db: &sqlx::SqlitePool, action: &str, user_id: Option<i64>, ip: &str, detail: &str) {
    let action = action.to_string();
    let ip = ip.to_string();
    let detail = detail.chars().take(200).collect::<String>();
    let db = db.clone();
    tokio::spawn(async move {
        let _ = sqlx::query(
            "INSERT INTO audit_log (action, user_id, ip, detail, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(action)
        .bind(user_id)
        .bind(ip)
        .bind(detail)
        .bind(crate::util::now_ms())
        .execute(&db)
        .await;
    });
}

/// 会话 cookie 组装
pub fn session_cookies(token: &str, csrf: &str, days: i64, secure: bool) -> Vec<String> {
    vec![
        crate::security::cookie_header(SESSION_COOKIE, token, Some(days * 86400), secure),
        crate::security::csrf_cookie_header(csrf, days * 86400, secure),
    ]
}

pub const _UNUSED: Option<Arc<()>> = None;

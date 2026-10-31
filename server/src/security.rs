use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::state::AppState;

// ==================== 密码哈希 ====================

/// OWASP 推荐参数：Argon2id, m=19MiB, t=2, p=1
fn argon2<'a>() -> Argon2<'a> {
    let params = Params::new(19 * 1024, 2, 1, None).expect("argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(argon2()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// 恒定时间字符串比较（防时序侧信道）
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ==================== JWT 会话 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    /// token 版本，用户改密/登出全部设备时 +1
    pub ver: i64,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
}

pub fn issue_token(
    secret: &[u8],
    user_id: i64,
    token_version: i64,
    days: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        ver: token_version,
        iat: now.timestamp(),
        exp: (now + chrono::Duration::days(days)).timestamp(),
        jti: uuid::Uuid::new_v4().to_string(),
    };
    encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
}

pub fn decode_token(secret: &[u8], token: &str) -> Option<Claims> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.leeway = 10;
    validation.set_required_spec_claims(&["exp", "sub"]);
    decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)
        .ok()
        .map(|d| d.claims)
}

// ==================== Cookie ====================

pub const SESSION_COOKIE: &str = "lv_session";
pub const CSRF_COOKIE: &str = "lv_csrf";

pub fn parse_cookies(headers: &HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for value in headers.get_all(header::COOKIE) {
        let Ok(s) = value.to_str() else { continue };
        for pair in s.split(';') {
            let pair = pair.trim();
            if let Some((k, v)) = pair.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    map
}

pub fn cookie_header(name: &str, value: &str, max_age: Option<i64>, secure: bool) -> String {
    let mut c = format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax");
    if let Some(age) = max_age {
        c.push_str(&format!("; Max-Age={age}"));
    }
    if secure {
        c.push_str("; Secure");
    }
    c
}

pub fn csrf_cookie_header(value: &str, max_age: i64, secure: bool) -> String {
    // CSRF cookie 需要被 JS 读取，故不带 HttpOnly
    let mut c = format!("{CSRF_COOKIE}={value}; Path=/; SameSite=Lax; Max-Age={max_age}");
    if secure {
        c.push_str("; Secure");
    }
    c
}

pub fn clear_cookie_header(name: &str, secure: bool) -> String {
    let mut c = format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        c.push_str("; Secure");
    }
    c
}

pub fn random_token(len: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

// ==================== 安全响应头 ====================

const CSP: &str = "default-src 'self'; \
img-src 'self' data: blob:; \
media-src 'self' blob:; \
style-src 'self' 'unsafe-inline'; \
script-src 'self'; \
connect-src 'self' ws: wss:; \
font-src 'self' data:; \
object-src 'none'; \
base-uri 'none'; \
frame-ancestors 'none'; \
form-action 'self'; \
upgrade-insecure-requests";

pub async fn security_headers(req: Request<axum::body::Body>, next: Next) -> Response {
    let secure = req
        .extensions()
        .get::<AppState>()
        .map(|s| s.cfg.secure_cookies)
        .unwrap_or(false);
    let is_doc = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/html"))
        .unwrap_or(false);

    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    h.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    h.insert(
        header::HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    h.insert(
        header::HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    h.insert(
        header::HeaderName::from_static("x-permitted-cross-domain-policies"),
        HeaderValue::from_static("none"),
    );
    if secure {
        h.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
        );
    }
    if is_doc {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    resp
}

// ==================== CSRF 双提交校验 ====================

pub async fn csrf_guard(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let unsafe_method = matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    if unsafe_method {
        // 跨站请求即使带 cookie 也必须先通过 Origin 校验
        if !origin_allowed(&req, &state) {
            return (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({
                    "error": {"code": "csrf", "message": "非法请求来源"}
                })),
            )
                .into_response();
        }

        let cookies = parse_cookies(req.headers());
        let cookie_token = cookies.get(CSRF_COOKIE).cloned().unwrap_or_default();
        let header_token = req
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if cookie_token.is_empty()
            || !constant_time_eq(cookie_token.as_bytes(), header_token.as_bytes())
        {
            return (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({
                    "error": {"code": "csrf", "message": "CSRF 校验失败，请刷新页面重试"}
                })),
            )
                .into_response();
        }
    }

    // 保证每个访问者都持有一个 CSRF token（前端从 cookie 读取后放入请求头）
    let needs_token = parse_cookies(req.headers())
        .get(CSRF_COOKIE)
        .map(|t| t.len() < 32)
        .unwrap_or(true);

    let mut resp = next.run(req).await;
    if needs_token {
        let token = random_token(24);
        if let Ok(v) = HeaderValue::from_str(&csrf_cookie_header(
            &token,
            60 * 60 * 24 * 7,
            state.cfg.secure_cookies,
        )) {
            resp.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    resp
}

/// 基于 HeaderMap 的 Origin 校验（WebSocket 握手复用）
pub fn origin_allowed_headers(headers: &HeaderMap, cfg: &crate::config::Config) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
            return matches!(site, "same-origin" | "same-site" | "none");
        }
        return true;
    };
    let origin = origin.trim_end_matches('/');
    if origin == cfg.public_url {
        return true;
    }
    if let Ok(url) = url_host(origin) {
        if let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
            if url == host {
                return true;
            }
        }
    }
    cfg.allowed_origins.iter().any(|o| o == origin)
}

pub fn origin_allowed(req: &Request<axum::body::Body>, state: &AppState) -> bool {
    // 无 Origin 头的同源表单/直接导航放行（浏览器同源 GET 不带 Origin）
    let Some(origin) = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
    else {
        // Sec-Fetch-Site 由现代浏览器提供，跨站请求显式拒绝
        if let Some(site) = req
            .headers()
            .get("sec-fetch-site")
            .and_then(|v| v.to_str().ok())
        {
            return matches!(site, "same-origin" | "same-site" | "none");
        }
        return true;
    };
    let origin = origin.trim_end_matches('/');
    if origin == state.cfg.public_url {
        return true;
    }
    // 同主机不同端口（开发环境 127.0.0.1 / localhost）
    if let Ok(url) = url_host(origin) {
        if let Some(host) = req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
        {
            if url == host {
                return true;
            }
        }
    }
    state.cfg.allowed_origins.iter().any(|o| o == origin)
}

fn url_host(origin: &str) -> Result<String, ()> {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .ok_or(())?;
    Ok(rest.split('/').next().ok_or(())?.to_string())
}

// ==================== 客户端 IP ====================

pub fn client_ip(headers: &HeaderMap, peer: Option<IpAddr>, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff.split(',').next() {
                let ip = first.trim();
                if !ip.is_empty() && ip.len() < 64 {
                    return ip.to_string();
                }
            }
        }
        if let Some(real) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            return real.trim().to_string();
        }
    }
    peer.map(|p| p.to_string())
        .unwrap_or_else(|| "0.0.0.0".into())
}

// ==================== 限流（滑动窗口） ====================

#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub max: u32,
    pub window: Duration,
}

impl Limit {
    pub const fn new(max: u32, secs: u64) -> Self {
        Self {
            max,
            window: Duration::from_secs(secs),
        }
    }
}

struct Bucket {
    start: Instant,
    count: u32,
}

pub struct RateLimiter {
    buckets: DashMap<String, Bucket>,
    last_gc: std::sync::Mutex<Instant>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            buckets: DashMap::new(),
            last_gc: std::sync::Mutex::new(Instant::now()),
        }
    }
}

impl RateLimiter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 返回 Ok(()) 或 需要等待的秒数
    pub fn check(&self, key: &str, limit: Limit) -> Result<(), u64> {
        let now = Instant::now();
        let mut entry = self.buckets.entry(key.to_string()).or_insert(Bucket {
            start: now,
            count: 0,
        });
        if now.duration_since(entry.start) >= limit.window {
            entry.start = now;
            entry.count = 1;
            return Ok(());
        }
        if entry.count >= limit.max {
            let elapsed = now.duration_since(entry.start).as_secs();
            return Err(limit.window.as_secs().saturating_sub(elapsed).max(1));
        }
        entry.count += 1;
        Ok(())
    }

    /// 周期性清理过期桶，避免内存无限增长
    pub fn gc(&self) {
        let now = Instant::now();
        {
            let mut last = match self.last_gc.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if now.duration_since(*last) < Duration::from_secs(120) {
                return;
            }
            *last = now;
        }
        self.buckets
            .retain(|_, b| now.duration_since(b.start) < Duration::from_secs(3600));
    }
}

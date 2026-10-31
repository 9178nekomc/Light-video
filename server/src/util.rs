use axum::Json;
use serde::Serialize;

/// 毫秒时间戳 -> ISO8601 UTC 字符串
pub fn iso(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn page_bounds(page: Option<i64>, limit: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).clamp(1, 10_000);
    let limit = limit.unwrap_or(20).clamp(1, 60);
    let offset = (page - 1).saturating_mul(limit);
    (limit, offset)
}

/// 文本清洗：去除控制字符、首尾空白，按字符（非字节）截断
pub fn clean_text(input: &str, max_chars: usize) -> String {
    let filtered: String = input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    let trimmed = filtered.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_chars).collect()
}

pub fn valid_username(name: &str) -> bool {
    let n = name.chars().count();
    if !(3..=20).contains(&n) {
        return false;
    }
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || !c.is_ascii())
}

pub fn valid_email(email: &str) -> bool {
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && local.len() <= 64
                && domain.contains('.')
                && domain.len() <= 190
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !email.contains(char::is_whitespace)
                && email.is_ascii()
        }
        _ => false,
    }
}

/// 口令强度：至少 8 位，且同时含字母与数字
pub fn password_strength(pw: &str) -> Result<(), &'static str> {
    if pw.chars().count() < 8 {
        return Err("密码至少需要 8 位字符");
    }
    if pw.len() > 200 {
        return Err("密码过长");
    }
    let has_digit = pw.chars().any(|c| c.is_ascii_digit());
    let has_alpha = pw.chars().any(|c| c.is_ascii_alphabetic());
    if !(has_digit && has_alpha) {
        return Err("密码需同时包含字母和数字");
    }
    if is_common_password(pw) {
        return Err("密码过于常见，请更换");
    }
    Ok(())
}

fn is_common_password(pw: &str) -> bool {
    const COMMON: &[&str] = &[
        "12345678",
        "password",
        "password1",
        "qwerty123",
        "abc12345",
        "11111111",
        "admin123",
        "iloveyou",
        "123456789",
        "a1234567",
        "passw0rd",
        "woaini1314",
    ];
    let lower = pw.to_ascii_lowercase();
    COMMON.iter().any(|c| *c == lower)
}

/// 文件存储 key 白名单校验，彻底阻断路径穿越
pub fn valid_storage_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 200 {
        return false;
    }
    if key.contains("..") || key.contains('\\') || key.starts_with('/') {
        return false;
    }
    key.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.')
}

/// 供 LIKE 使用：转义通配符，返回 owned 字符串
pub fn escape_like_owned(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn json_str_array(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".into())
}

pub fn parse_str_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

/// 统一成功响应包装：{ "ok": true, "data": ... }
#[derive(Serialize)]
pub struct ApiOk<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

pub fn ok<T: Serialize>(data: T) -> Json<ApiOk<T>> {
    Json(ApiOk { ok: true, data })
}

/// 分页响应
#[derive(Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub page: i64,
    pub limit: i64,
    pub has_more: bool,
}

pub fn page<T: Serialize>(items: Vec<T>, page: i64, limit: i64, has_more: bool) -> Page<T> {
    Page {
        items,
        page,
        limit,
        has_more,
    }
}

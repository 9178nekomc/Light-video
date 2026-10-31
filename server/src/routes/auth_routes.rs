use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{audit, session_cookies, MaybeUser, RequiredUser};
use crate::error::{AppError, AppResult};
use crate::models::{UserRow, USER_COLS};
use crate::routes::append_cookies;
use crate::security::{
    client_ip, hash_password, random_token, verify_password, CSRF_COOKIE, SESSION_COOKIE,
};
use crate::state::AppState;
use crate::util::{clean_text, iso, password_strength, valid_email, valid_username};

fn client_ip_of(headers: &HeaderMap, peer: SocketAddr, st: &AppState) -> String {
    client_ip(headers, Some(peer.ip()), st.cfg.trust_proxy)
}

fn user_json(u: &UserRow) -> serde_json::Value {
    json!({
        "id": u.id,
        "username": u.username,
        "email": u.email,
        "display_name": if u.display_name.is_empty() { u.username.clone() } else { u.display_name.clone() },
        "avatar_url": u.avatar_url(),
        "bio": u.bio,
        "role": u.role,
        "created_at": iso(u.created_at),
    })
}

fn login_response(u: &UserRow, token: String, csrf: &str, st: &AppState) -> Response {
    let days = st.cfg.session_days;
    let resp = (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "data": { "user": user_json(u), "csrf": csrf }
        })),
    )
        .into_response();
    append_cookies(
        resp,
        session_cookies(&token, csrf, days, st.cfg.secure_cookies),
    )
}

// ==================== 注册 ====================

#[derive(Deserialize)]
pub struct RegisterReq {
    pub username: String,
    pub email: String,
    pub password: String,
}

pub async fn register(
    State(st): State<AppState>,
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<RegisterReq>,
) -> AppResult<Response> {
    let ip = client_ip_of(&headers, ci, &st);
    st.rate_limit("register", &ip, 8, 3600)?;

    let username = clean_text(&req.username, 20);
    let email = clean_text(&req.email, 190).to_ascii_lowercase();
    if !valid_username(&username) {
        return Err(AppError::BadRequest(
            "用户名需 3-20 位，仅支持中英文、数字、下划线与短横线".into(),
        ));
    }
    if !valid_email(&email) {
        return Err(AppError::BadRequest("邮箱格式不正确".into()));
    }
    password_strength(&req.password).map_err(|e| AppError::BadRequest(e.into()))?;

    let hash = hash_password(&req.password)
        .map_err(|e| AppError::Other(anyhow::anyhow!("hash error: {e}")))?;
    let now = crate::util::now_ms();

    let res = sqlx::query(
        "INSERT INTO users (username, email, password_hash, display_name, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&username)
    .bind(&email)
    .bind(&hash)
    .bind(&username)
    .bind(now)
    .bind(now)
    .execute(&st.db)
    .await;

    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::Conflict("用户名或邮箱已被占用".into()));
        }
    }
    res?;

    let id = sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE username = ?")
        .bind(&username)
        .fetch_one(&st.db)
        .await?;
    let user = fetch_user(&st, id).await?;

    let token = crate::security::issue_token(&st.cfg.jwt_secret, id, 1, st.cfg.session_days)
        .map_err(|e| AppError::Other(e.into()))?;
    let csrf = random_token(24);
    audit(
        &st.db,
        "register",
        Some(id),
        &ip,
        &format!("user={username}"),
    );
    Ok(login_response(&user, token, &csrf, &st))
}

// ==================== 登录 ====================

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub remember: bool,
}

pub async fn login(
    State(st): State<AppState>,
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> AppResult<Response> {
    let ip = client_ip_of(&headers, ci, &st);
    let account = clean_text(&req.username, 190);

    // 双维度限流：IP 防撞库，账号防定向爆破
    st.rate_limit("login_ip", &ip, 20, 300)?;
    st.rate_limit("login_acc", &account.to_ascii_lowercase(), 8, 300)?;

    if req.password.len() > 200 {
        return Err(AppError::Unauthorized);
    }

    let row = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLS} FROM users WHERE username = ? OR email = ?"
    ))
    .bind(&account)
    .bind(&account)
    .fetch_optional(&st.db)
    .await?;

    // 无论账号是否存在都执行一次哈希校验耗时相近的分支，降低用户枚举风险
    let dummy = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$8Q0m8f0kQ0m8f0kQ0m8f0kQ0m8f0kQ0m8f0kQ0m8f0k";
    let (user, valid) = match &row {
        Some(u) => (Some(u), verify_password(&req.password, &u.password_hash)),
        None => {
            let _ = verify_password(&req.password, dummy);
            (None, false)
        }
    };

    let Some(user) = user.filter(|_| valid) else {
        audit(
            &st.db,
            "login_failed",
            row.as_ref().map(|u| u.id),
            &ip,
            &account,
        );
        return Err(AppError::Unauthorized);
    };

    if user.is_banned != 0 {
        audit(&st.db, "login_banned", Some(user.id), &ip, &account);
        return Err(AppError::Forbidden("账号已被封禁".into()));
    }

    let days = if req.remember { st.cfg.session_days } else { 1 };
    let token = crate::security::issue_token(&st.cfg.jwt_secret, user.id, user.token_version, days)
        .map_err(|e| AppError::Other(e.into()))?;
    let csrf = random_token(24);
    st.sessions.put(crate::models::AuthUser {
        id: user.id,
        username: user.username.clone(),
        role: user.role.clone(),
        token_version: user.token_version,
        is_banned: false,
    });
    audit(&st.db, "login_ok", Some(user.id), &ip, "");
    Ok(login_response(&user, token, &csrf, &st))
}

// ==================== 登出 ====================

pub async fn logout(State(st): State<AppState>, user: RequiredUser) -> Response {
    st.sessions.invalidate(user.0.id);
    let resp = Json(json!({"ok": true, "data": {"logged_out": true}})).into_response();
    let mut cookies = vec![crate::security::clear_cookie_header(
        SESSION_COOKIE,
        st.cfg.secure_cookies,
    )];
    cookies.push(crate::security::clear_cookie_header(
        CSRF_COOKIE,
        st.cfg.secure_cookies,
    ));
    append_cookies(resp, cookies)
}

// ==================== 当前用户 ====================

pub async fn me(
    State(st): State<AppState>,
    MaybeUser(user): MaybeUser,
) -> AppResult<Json<serde_json::Value>> {
    let Some(u) = user else {
        return Ok(Json(json!({"ok": true, "data": null})));
    };
    let row = fetch_user(&st, u.id).await?;
    let videos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM videos WHERE user_id = ?")
        .bind(u.id)
        .fetch_one(&st.db)
        .await?;
    let friends: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM friendships WHERE status='accepted' AND (requester_id = ? OR addressee_id = ?)",
    )
    .bind(u.id)
    .bind(u.id)
    .fetch_one(&st.db)
    .await?;
    let unread_msg: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE receiver_id = ? AND is_read = 0")
            .bind(u.id)
            .fetch_one(&st.db)
            .await?;
    let unread_notif: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = ? AND is_read = 0")
            .bind(u.id)
            .fetch_one(&st.db)
            .await?;

    Ok(Json(json!({
        "ok": true,
        "data": {
            "user": user_json(&row),
            "stats": {"videos": videos, "friends": friends, "unread_messages": unread_msg, "unread_notifications": unread_notif},
        }
    })))
}

// ==================== 修改密码 ====================

#[derive(Deserialize)]
pub struct PasswordReq {
    pub current: String,
    pub new: String,
}

pub async fn change_password(
    State(st): State<AppState>,
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: RequiredUser,
    Json(req): Json<PasswordReq>,
) -> AppResult<Response> {
    let ip = client_ip_of(&headers, ci, &st);
    st.rate_limit("pwd", &format!("{}:{}", user.0.id, ip), 5, 900)?;
    password_strength(&req.new).map_err(|e| AppError::BadRequest(e.into()))?;

    let row = fetch_user(&st, user.0.id).await?;
    if !verify_password(&req.current, &row.password_hash) {
        audit(&st.db, "pwd_change_failed", Some(user.0.id), &ip, "");
        return Err(AppError::Forbidden("当前密码不正确".into()));
    }
    let hash =
        hash_password(&req.new).map_err(|e| AppError::Other(anyhow::anyhow!("hash error: {e}")))?;
    sqlx::query("UPDATE users SET password_hash = ?, token_version = token_version + 1, updated_at = ? WHERE id = ?")
        .bind(&hash)
        .bind(crate::util::now_ms())
        .bind(user.0.id)
        .execute(&st.db)
        .await?;

    // 使所有旧会话失效，然后给当前设备签发新 token
    st.sessions.invalidate(user.0.id);
    let new_ver = user.0.token_version + 1;
    let token =
        crate::security::issue_token(&st.cfg.jwt_secret, user.0.id, new_ver, st.cfg.session_days)
            .map_err(|e| AppError::Other(e.into()))?;
    let csrf = random_token(24);
    audit(&st.db, "pwd_changed", Some(user.0.id), &ip, "");

    let user2 = fetch_user(&st, user.0.id).await?;
    let resp = (
        StatusCode::OK,
        Json(json!({"ok": true, "data": {"user": user_json(&user2)}})),
    )
        .into_response();
    Ok(append_cookies(
        resp,
        session_cookies(&token, &csrf, st.cfg.session_days, st.cfg.secure_cookies),
    ))
}

// ==================== 资料 ====================

#[derive(Deserialize)]
pub struct ProfileReq {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
}

pub async fn update_profile(
    State(st): State<AppState>,
    user: RequiredUser,
    Json(req): Json<ProfileReq>,
) -> AppResult<Json<serde_json::Value>> {
    if let Some(name) = req.display_name {
        let name = clean_text(&name, 24);
        if name.is_empty() {
            return Err(AppError::BadRequest("昵称不能为空".into()));
        }
        sqlx::query("UPDATE users SET display_name = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(crate::util::now_ms())
            .bind(user.0.id)
            .execute(&st.db)
            .await?;
    }
    if let Some(bio) = req.bio {
        let bio = clean_text(&bio, 200);
        sqlx::query("UPDATE users SET bio = ?, updated_at = ? WHERE id = ?")
            .bind(&bio)
            .bind(crate::util::now_ms())
            .bind(user.0.id)
            .execute(&st.db)
            .await?;
    }
    let row = fetch_user(&st, user.0.id).await?;
    Ok(Json(json!({"ok": true, "data": user_json(&row)})))
}

// ==================== 头像 ====================

pub async fn update_avatar(
    State(st): State<AppState>,
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: RequiredUser,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let ip = client_ip_of(&headers, ci, &st);
    st.rate_limit("avatar", &format!("{}", user.0.id), 10, 3600)?;

    let mut data: Option<bytes::Bytes> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("表单解析失败".into()))?
    {
        if field.name() == Some("avatar") {
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::BadRequest("读取文件失败".into()))?;
            if bytes.len() as i64 > st.cfg.max_cover_bytes {
                return Err(AppError::PayloadTooLarge);
            }
            data = Some(bytes);
            break;
        }
    }
    let data = data.ok_or_else(|| AppError::BadRequest("缺少 avatar 字段".into()))?;
    let Some((kind, _mime, ext)) = crate::media::sniff(&data) else {
        return Err(AppError::UnsupportedMedia);
    };
    if kind != crate::media::MediaKind::Image {
        return Err(AppError::UnsupportedMedia);
    }
    if data.len() > 4 * 1024 * 1024 {
        return Err(AppError::PayloadTooLarge);
    }

    let key = format!("avatars/{}.{}", uuid::Uuid::new_v4(), ext);
    tokio::fs::write(st.cfg.data_dir.join(&key), &data)
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    let old = sqlx::query_scalar::<_, String>("SELECT avatar_key FROM users WHERE id = ?")
        .bind(user.0.id)
        .fetch_one(&st.db)
        .await?;
    sqlx::query("UPDATE users SET avatar_key = ?, updated_at = ? WHERE id = ?")
        .bind(&key)
        .bind(crate::util::now_ms())
        .bind(user.0.id)
        .execute(&st.db)
        .await?;

    if crate::util::valid_storage_key(&old) {
        let _ = tokio::fs::remove_file(st.cfg.data_dir.join(&old)).await;
    }
    audit(&st.db, "avatar_update", Some(user.0.id), &ip, "");
    Ok(Json(
        json!({"ok": true, "data": {"avatar_url": format!("/media/{key}")}}),
    ))
}

async fn fetch_user(st: &AppState, id: i64) -> AppResult<UserRow> {
    sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLS} FROM users WHERE id = ?"))
        .bind(id)
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".into()))
}

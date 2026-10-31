//! 端到端冒烟测试：直接驱动真实 Router + 真实 SQLite（临时目录），
//! 覆盖认证/CSRF/分片上传/播放地址/弹幕/评论/点赞收藏/收藏列表/越权。

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use lightvideo::config::Config;
use lightvideo::state::AppState;

struct Ctx {
    app: Router,
    dir: std::path::PathBuf,
    csrf: String,
    session: String,
}

fn test_config(dir: &std::path::Path) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        public_url: "http://127.0.0.1:3000".into(),
        data_dir: dir.to_path_buf(),
        db_path: dir.join("test.db"),
        jwt_secret: Arc::new(b"test-secret-key-at-least-32-bytes-long!!".to_vec()),
        session_days: 7,
        secure_cookies: false,
        allowed_origins: vec![],
        trust_proxy: false,
        max_video_bytes: 64 * 1024 * 1024,
        max_cover_bytes: 8 * 1024 * 1024,
        chunk_size: 1024 * 1024,
        ffmpeg_path: "ffmpeg".into(),
        ffprobe_path: "ffprobe".into(),
        max_upload_sessions: 4,
    }
}

async fn setup() -> Ctx {
    setup_with_chunk(1024 * 1024).await
}

async fn setup_with_chunk(chunk_size: i64) -> Ctx {
    let dir = std::env::temp_dir().join(format!("lv-test-{}", uuid::Uuid::new_v4()));
    for sub in ["media", "hls", "tmp", "covers", "avatars"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let mut config = test_config(&dir);
    config.chunk_size = chunk_size;
    let cfg = Arc::new(config);
    let pool = lightvideo::db::connect(&cfg.db_path, 4).await.unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let state = AppState::new(cfg, pool, tx, false);
    let app = lightvideo::api_router(state)
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 45678))));
    Ctx {
        app,
        dir,
        csrf: String::new(),
        session: String::new(),
    }
}

struct Resp {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
}

async fn call(
    ctx: &Ctx,
    method: Method,
    uri: &str,
    body: Option<Value>,
    with_auth: bool,
    with_csrf: bool,
) -> Resp {
    let mut builder = Request::builder().method(method).uri(uri);
    let mut cookie = Vec::new();
    if with_auth && !ctx.session.is_empty() {
        cookie.push(format!("lv_session={}", ctx.session));
    }
    cookie.push(format!(
        "lv_csrf={}",
        if ctx.csrf.is_empty() {
            "seed"
        } else {
            &ctx.csrf
        }
    ));
    builder = builder.header(header::COOKIE, cookie.join("; "));
    if with_csrf {
        builder = builder.header("x-csrf-token", ctx.csrf.clone());
    }
    let body = match body {
        Some(v) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let req = builder.body(body).unwrap();
    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Resp {
        status,
        headers,
        body,
    }
}

fn set_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    for v in headers.get_all(header::SET_COOKIE) {
        let s = v.to_str().ok()?;
        if let Some(rest) = s.strip_prefix(&format!("{name}=")) {
            return Some(rest.split(';').next().unwrap_or("").to_string());
        }
    }
    None
}

fn mp4_stub(size: usize) -> Vec<u8> {
    let mut v = vec![0u8; size];
    v[0..4].copy_from_slice(&32u32.to_be_bytes());
    v[4..8].copy_from_slice(b"ftyp");
    v[8..12].copy_from_slice(b"isom");
    v
}

#[tokio::test]
async fn full_api_flow() {
    let mut ctx = setup().await;

    // 1. 健康检查
    let r = call(&ctx, Method::GET, "/api/health", None, false, false).await;
    assert_eq!(r.status, StatusCode::OK, "health");
    assert_eq!(r.body["status"], "ok");

    // 2. 首个 GET 会下发 CSRF cookie
    let csrf = set_cookie(&r.headers, "lv_csrf").expect("csrf cookie");
    ctx.csrf = csrf;

    // 3. 缺少 CSRF 头的写请求必须被拒绝
    let r = call(
        &ctx,
        Method::POST,
        "/api/auth/register",
        Some(json!({"username":"alice","email":"alice@example.com","password":"Passw0rd1"})),
        false,
        false,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "csrf guard");

    // 4. 正常注册
    let r = call(
        &ctx,
        Method::POST,
        "/api/auth/register",
        Some(json!({"username":"alice","email":"alice@example.com","password":"Passw0rd1"})),
        false,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "register: {:?}", r.body);
    assert_eq!(r.body["data"]["user"]["username"], "alice");
    ctx.session = set_cookie(&r.headers, "lv_session").expect("session cookie");

    // 5. 重复注册 → 409
    let r = call(
        &ctx,
        Method::POST,
        "/api/auth/register",
        Some(json!({"username":"alice","email":"alice@example.com","password":"Passw0rd1"})),
        false,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // 6. 弱密码 → 400
    let r = call(
        &ctx,
        Method::POST,
        "/api/auth/register",
        Some(json!({"username":"bob","email":"bob@example.com","password":"12345678"})),
        false,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // 7. 当前用户
    let r = call(&ctx, Method::GET, "/api/auth/me", None, true, false).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["data"]["user"]["username"], "alice");

    // 8. 未登录访问受保护接口 → 401
    let r = call(&ctx, Method::POST, "/api/videos/1/like", None, false, true).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    // 9. 分片上传：init → 3 个分片 → complete
    let data = mp4_stub(2500);
    let r = call(
        &ctx,
        Method::POST,
        "/api/uploads/init",
        Some(json!({"filename":"demo.mp4","total_size":data.len()})),
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "init: {:?}", r.body);
    let upload_id = r.body["data"]["upload_id"].as_str().unwrap().to_string();
    let chunk = r.body["data"]["chunk_size"].as_i64().unwrap() as usize;
    assert_eq!(r.body["data"]["total_chunks"], 1);

    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/uploads/{upload_id}/chunk/0"))
        .header(
            header::COOKIE,
            format!("lv_session={}; lv_csrf={}", ctx.session, ctx.csrf),
        )
        .header("x-csrf-token", &ctx.csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(data.clone()))
        .unwrap();
    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "chunk upload");
    assert!(chunk >= 1024);

    let r = call(
        &ctx,
        Method::POST,
        &format!("/api/uploads/{upload_id}/complete"),
        Some(json!({"title":"测试视频","description":"冒烟测试","category":"tech","tags":["rust","test"]})),
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "complete: {:?}", r.body);
    let video_id = r.body["data"]["video_id"].as_i64().unwrap();
    assert!(video_id > 0);

    // 10. 列表
    let r = call(
        &ctx,
        Method::GET,
        "/api/videos?limit=10",
        None,
        false,
        false,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(r.body["data"]["items"][0]["title"], "测试视频");

    // 11. 详情 + 流地址
    let r = call(
        &ctx,
        Method::GET,
        &format!("/api/videos/{video_id}"),
        None,
        true,
        false,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let stream = r.body["data"]["stream_url"].as_str().unwrap().to_string();
    assert!(stream.starts_with("/media/"));
    assert_eq!(r.body["data"]["is_owner"], true);

    // 12. Range 请求返回 206 且长度正确
    let req = Request::builder()
        .method(Method::GET)
        .uri(&stream)
        .header(header::RANGE, "bytes=0-99")
        .body(Body::empty())
        .unwrap();
    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        resp.headers()[header::CONTENT_RANGE],
        format!("bytes 0-99/2500")
    );
    assert_eq!(resp.headers()[header::ACCEPT_RANGES], "bytes");

    // 13. 路径穿越必须失败
    let req = Request::builder()
        .method(Method::GET)
        .uri("/media/..%2F..%2Ftest.db")
        .body(Body::empty())
        .unwrap();
    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::OK, "path traversal blocked");

    // 13.1 敏感文件绝不能通过媒体路由下载（数据库 / JWT 密钥）
    for probe in [
        "/media/lightvideo.db",
        "/media/jwt.key",
        "/media/media/../../lightvideo.db",
    ] {
        let req = Request::builder()
            .method(Method::GET)
            .uri(probe)
            .body(Body::empty())
            .unwrap();
        let resp = ctx.app.clone().oneshot(req).await.unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::OK,
            "sensitive file leaked: {probe}"
        );
    }

    // 14. 弹幕：非法颜色回退为白色
    let r = call(
        &ctx,
        Method::POST,
        &format!("/api/videos/{video_id}/danmaku"),
        Some(json!({"content":"第一条弹幕","time_ms":1200,"color":"javascript:alert(1)","mode":"top","font_size":99})),
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "danmaku: {:?}", r.body);
    assert_eq!(r.body["data"]["color"], "#ffffff");
    assert_eq!(r.body["data"]["font_size"], 48);
    assert_eq!(r.body["data"]["mode"], "top");

    let r = call(
        &ctx,
        Method::GET,
        &format!("/api/videos/{video_id}/danmaku"),
        None,
        false,
        false,
    )
    .await;
    assert_eq!(r.body["data"].as_array().unwrap().len(), 1);

    // 14.1 相关推荐：随机抽取，不包含自身
    let r = call(&ctx, Method::GET, &format!("/api/videos/{video_id}/related"), None, false, false).await;
    assert_eq!(r.status, StatusCode::OK);
    let list = r.body["data"].as_array().unwrap();
    assert!(list.len() <= 10);
    assert!(!list.iter().any(|v| v["id"].as_i64() == Some(video_id)), "related contains self");

    // 15. 评论
    let r = call(
        &ctx,
        Method::POST,
        &format!("/api/videos/{video_id}/comments"),
        Some(json!({"content":"很不错的视频"})),
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let comment_id = r.body["data"]["id"].as_i64().unwrap();

    let r = call(
        &ctx,
        Method::GET,
        &format!("/api/videos/{video_id}/comments"),
        None,
        false,
        false,
    )
    .await;
    assert_eq!(r.body["data"]["total"], 1);

    // 16. 点赞 / 收藏 / 收藏列表
    let r = call(
        &ctx,
        Method::POST,
        &format!("/api/videos/{video_id}/like"),
        None,
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["data"]["liked"], true);
    assert_eq!(r.body["data"]["likes_count"], 1);

    let r = call(
        &ctx,
        Method::POST,
        &format!("/api/videos/{video_id}/favorite"),
        None,
        true,
        true,
    )
    .await;
    assert_eq!(r.body["data"]["favorited"], true);

    let r = call(&ctx, Method::GET, "/api/me/favorites", None, true, false).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["data"]["items"].as_array().unwrap().len(), 1);

    // 17. 越权删除：另一个用户不能删除别人的视频
    let r = call(&ctx, Method::POST, "/api/auth/logout", None, true, true).await;
    assert_eq!(r.status, StatusCode::OK);

    let r = call(
        &ctx,
        Method::POST,
        "/api/auth/register",
        Some(json!({"username":"mallory","email":"m@example.com","password":"Passw0rd2"})),
        false,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    ctx.session = set_cookie(&r.headers, "lv_session").unwrap();

    let r = call(
        &ctx,
        Method::DELETE,
        &format!("/api/videos/{video_id}"),
        None,
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "owner check");

    let r = call(
        &ctx,
        Method::DELETE,
        &format!("/api/comments/{comment_id}"),
        None,
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "comment owner check");

    // 18. 非法分区被拒绝
    let r = call(
        &ctx,
        Method::PATCH,
        &format!("/api/videos/{video_id}"),
        Some(json!({"category":"hacking"})),
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // 清理
    let _ = std::fs::remove_dir_all(&ctx.dir);
}

#[tokio::test]
async fn ws_hub_broadcast() {
    use lightvideo::ws::Hub;

    let hub = Hub::new();
    let (tx1, mut rx1) = tokio::sync::mpsc::channel(8);
    let (tx2, mut rx2) = tokio::sync::mpsc::channel(8);
    hub.join("video:1", 1, tx1);
    hub.join("video:1", 2, tx2);

    assert_eq!(hub.presence("video:1"), 2);
    let sent = hub.broadcast("video:1", r#"{"type":"ping"}"#);
    assert_eq!(sent, 2);
    assert!(rx1.recv().await.is_some());
    assert!(rx2.recv().await.is_some());

    hub.leave("video:1", 1);
    assert_eq!(hub.presence("video:1"), 1);
    hub.leave_all(2);
    assert_eq!(hub.presence("video:1"), 0);
}

#[tokio::test]
async fn login_and_rate_limit() {
    let mut ctx = setup().await;
    let r = call(&ctx, Method::GET, "/api/health", None, false, false).await;
    ctx.csrf = set_cookie(&r.headers, "lv_csrf").unwrap();

    let r = call(
        &ctx,
        Method::POST,
        "/api/auth/register",
        Some(json!({"username":"carol","email":"carol@example.com","password":"Passw0rd3"})),
        false,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);

    // 错误密码连续尝试应最终触发 429（账号维度 8 次/5 分钟）
    let mut saw_429 = false;
    for _ in 0..12 {
        let r = call(
            &ctx,
            Method::POST,
            "/api/auth/login",
            Some(json!({"username":"carol","password":"wrong-pass-1"})),
            false,
            true,
        )
        .await;
        if r.status == StatusCode::TOO_MANY_REQUESTS {
            saw_429 = true;
            assert!(r.headers.contains_key(header::RETRY_AFTER));
            break;
        }
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    assert!(saw_429, "login brute force should be rate limited");

    let _ = std::fs::remove_dir_all(&ctx.dir);
}


/// 回归：分片与封面必须能超过 axum 默认的 2MiB body 上限
#[tokio::test]
async fn accepts_chunks_and_covers_over_two_mib() {
    let mut ctx = setup_with_chunk(4 * 1024 * 1024).await;

    let r = call(&ctx, Method::GET, "/api/health", None, false, false).await;
    ctx.csrf = set_cookie(&r.headers, "lv_csrf").unwrap();
    let r = call(
        &ctx,
        Method::POST,
        "/api/auth/register",
        Some(json!({"username":"bigio","email":"bigio@example.com","password":"Passw0rd9"})),
        false,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    ctx.session = set_cookie(&r.headers, "lv_session").unwrap();

    let data = mp4_stub(6 * 1024 * 1024);
    let r = call(
        &ctx,
        Method::POST,
        "/api/uploads/init",
        Some(json!({"filename":"big.mp4","total_size":data.len()})),
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "init: {:?}", r.body);
    let upload_id = r.body["data"]["upload_id"].as_str().unwrap().to_string();
    assert_eq!(r.body["data"]["total_chunks"], 2);

    let chunk = 4 * 1024 * 1024usize;
    for (idx, part) in [&data[..chunk], &data[chunk..]].iter().enumerate() {
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/uploads/{upload_id}/chunk/{idx}"))
            .header(header::COOKIE, format!("lv_session={}; lv_csrf={}", ctx.session, ctx.csrf))
            .header("x-csrf-token", &ctx.csrf)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(part.to_vec()))
            .unwrap();
        let resp = ctx.app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT, "chunk {idx} rejected as too large");
    }

    let r = call(
        &ctx,
        Method::POST,
        &format!("/api/uploads/{upload_id}/complete"),
        Some(json!({"title":"大文件测试","category":"tech"})),
        true,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "complete: {:?}", r.body);

    // 3MiB 的 PNG 封面同样要能通过
    let mut png = vec![0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.resize(3 * 1024 * 1024, 0);
    let boundary = "----lvtest";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"cover\"; filename=\"c.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/uploads/cover")
        .header(header::COOKIE, format!("lv_session={}; lv_csrf={}", ctx.session, ctx.csrf))
        .header("x-csrf-token", &ctx.csrf)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "cover over 2MiB rejected");

    let _ = std::fs::remove_dir_all(&ctx.dir);
}

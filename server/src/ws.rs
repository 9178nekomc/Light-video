use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::auth::resolve_session;
use crate::models::AuthUser;
use crate::security::origin_allowed_headers;
use crate::state::AppState;

// ==================== 服务端 → 客户端 ====================

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    Hello {
        user: Option<PublicUser>,
        server_time: i64,
    },
    Danmaku {
        data: serde_json::Value,
    },
    Chat {
        data: serde_json::Value,
    },
    ChatAck {
        id: i64,
        client_id: String,
        created_at: String,
    },
    Notification {
        data: serde_json::Value,
    },
    Presence {
        room: String,
        count: usize,
    },
    Typing {
        from: i64,
        username: String,
    },
    Error {
        code: String,
        message: String,
    },
    Pong {
        t: i64,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicUser {
    pub id: i64,
    pub username: String,
}

// ==================== 客户端 → 服务端 ====================

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    Join {
        video_id: i64,
    },
    Leave {
        video_id: i64,
    },
    Danmaku {
        video_id: i64,
        content: String,
        #[serde(default)]
        time_ms: i64,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        font_size: Option<i64>,
    },
    Chat {
        to: i64,
        content: String,
        #[serde(default)]
        client_id: String,
        #[serde(default)]
        video_id: Option<i64>,
    },
    Typing {
        to: i64,
    },
    Ping {
        #[serde(default)]
        t: i64,
    },
}

// ==================== 房间广播中心 ====================

#[derive(Default)]
pub struct Hub {
    rooms: DashMap<String, DashMap<u64, mpsc::Sender<String>>>,
    conn_rooms: DashMap<u64, Vec<String>>,
    next_id: AtomicU64,
}

impl Hub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn next_conn_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn join(&self, room: &str, conn: u64, tx: mpsc::Sender<String>) {
        self.rooms
            .entry(room.to_string())
            .or_default()
            .insert(conn, tx);
        self.conn_rooms
            .entry(conn)
            .or_default()
            .push(room.to_string());
    }

    pub fn leave(&self, room: &str, conn: u64) {
        if let Some(r) = self.rooms.get(room) {
            r.remove(&conn);
            if r.is_empty() {
                drop(r);
                self.rooms.remove(room);
            }
        }
    }

    pub fn leave_all(&self, conn: u64) {
        if let Some((_, rooms)) = self.conn_rooms.remove(&conn) {
            for room in rooms {
                self.leave(&room, conn);
            }
        }
    }

    pub fn presence(&self, room: &str) -> usize {
        self.rooms.get(room).map(|r| r.len()).unwrap_or(0)
    }

    /// 广播：慢客户端直接丢弃（弹幕场景宁可丢帧也不拖垮服务）
    pub fn broadcast(&self, room: &str, payload: &str) -> usize {
        let mut sent = 0;
        if let Some(members) = self.rooms.get(room) {
            for entry in members.iter() {
                if entry.value().try_send(payload.to_string()).is_ok() {
                    sent += 1;
                }
            }
        }
        sent
    }

    pub fn broadcast_event(&self, room: &str, event: &ServerEvent) {
        match serde_json::to_string(event) {
            Ok(s) => {
                self.broadcast(room, &s);
            }
            Err(e) => tracing::error!("序列化广播事件失败: {e}"),
        }
    }

    pub fn send_to_user(&self, user_id: i64, event: &ServerEvent) -> bool {
        let room = format!("user:{user_id}");
        match serde_json::to_string(event) {
            Ok(s) => self.broadcast(&room, &s) > 0,
            Err(_) => false,
        }
    }

    pub fn online_users(&self) -> usize {
        self.rooms
            .iter()
            .filter(|r| r.key().starts_with("user:"))
            .map(|r| r.value().len())
            .sum()
    }
}

// ==================== 握手 ====================

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    // WS 不受同源策略保护，必须自行校验 Origin，防止跨站 WebSocket 劫持
    if !origin_allowed_headers(&headers, &state.cfg) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }

    let user = resolve_session(&state, &headers).await;
    let room = q
        .get("video_id")
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|id| *id > 0 && *id < i64::MAX / 2);

    ws.max_message_size(64 * 1024)
        .max_frame_size(32 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state, user, room))
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    user: Option<AuthUser>,
    initial_room: Option<i64>,
) {
    let conn_id = state.hub.next_conn_id();
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<String>(256);

    // 已登录用户自动订阅个人频道（私信 / 通知 / 好友请求）
    if let Some(u) = &user {
        state
            .hub
            .join(&format!("user:{}", u.id), conn_id, tx.clone());
    }

    let hello = ServerEvent::Hello {
        user: user.as_ref().map(|u| PublicUser {
            id: u.id,
            username: u.username.clone(),
        }),
        server_time: crate::util::now_ms(),
    };
    if let Ok(s) = serde_json::to_string(&hello) {
        let _ = tx.try_send(s);
    }

    if let Some(vid) = initial_room {
        let room = format!("video:{vid}");
        state.hub.join(&room, conn_id, tx.clone());
        let count = state.hub.presence(&room);
        state.hub.broadcast_event(
            &room,
            &ServerEvent::Presence {
                room: room.clone(),
                count,
            },
        );
    }

    // 写协程：独占 sink
    let writer = tokio::spawn(async move {
        while let Some(payload) = rx.recv().await {
            if sink.send(Message::Text(payload.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let mut last_seen = Instant::now();
    let mut joined_videos: Vec<i64> = initial_room.into_iter().collect();
    // 简易令牌桶：弹幕 5 条 / 5 秒，聊天 20 条 / 10 秒
    let mut dm_tokens: f64 = 5.0;
    let mut chat_tokens: f64 = 20.0;
    let mut tick = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                // 令牌按秒回填充
                dm_tokens = (dm_tokens + 0.8).min(5.0);
                chat_tokens = (chat_tokens + 2.0).min(20.0);
                if last_seen.elapsed() > Duration::from_secs(75) {
                    break;
                }
            }
            incoming = stream.next() => {
                let Some(Ok(msg)) = incoming else { break };
                last_seen = Instant::now();
                match msg {
                    Message::Text(text) => {
                        if text.len() > 16 * 1024 {
                            continue;
                        }
                        let Ok(ev) = serde_json::from_str::<ClientEvent>(&text) else {
                            if let Some(f) = err_frame("bad_event", "无法解析的消息") { let _ = tx.try_send(f); }
                            continue;
                        };
                        match ev {
                            ClientEvent::Ping { t } => {
                                if let Ok(s) = serde_json::to_string(&ServerEvent::Pong { t }) {
                                    let _ = tx.try_send(s);
                                }
                            }
                            ClientEvent::Join { video_id } => {
                                if joined_videos.len() >= 3 || joined_videos.contains(&video_id) {
                                    continue;
                                }
                                if !video_exists(&state, video_id).await { continue; }
                                joined_videos.push(video_id);
                                let room = format!("video:{video_id}");
                                state.hub.join(&room, conn_id, tx.clone());
                                let count = state.hub.presence(&room);
                                state.hub.broadcast_event(&room, &ServerEvent::Presence { room: room.clone(), count });
                            }
                            ClientEvent::Leave { video_id } => {
                                joined_videos.retain(|v| *v != video_id);
                                let room = format!("video:{video_id}");
                                state.hub.leave(&room, conn_id);
                                let count = state.hub.presence(&room);
                                state.hub.broadcast_event(&room, &ServerEvent::Presence { room: room.clone(), count });
                            }
                            ClientEvent::Danmaku { video_id, content, time_ms, color, mode, font_size } => {
                                let Some(u) = &user else {
                                    if let Some(f) = err_frame("unauthorized", "登录后才能发送弹幕") { let _ = tx.try_send(f); }
                                    continue;
                                };
                                if dm_tokens < 1.0 {
                                    if let Some(f) = err_frame("rate_limited", "弹幕发得太快了") { let _ = tx.try_send(f); }
                                    continue;
                                }
                                dm_tokens -= 1.0;
                                match crate::routes::danmaku::insert_danmaku(
                                    &state, u, video_id, &content, time_ms, color, mode, font_size,
                                ).await {
                                    Ok(data) => {
                                        state.hub.broadcast_event(&format!("video:{video_id}"), &ServerEvent::Danmaku { data });
                                    }
                                    Err(e) => {
                                        if let Some(f) = err_frame("danmaku_failed", &e.to_string()) { let _ = tx.try_send(f); }
                                    }
                                }
                            }
                            ClientEvent::Chat { to, content, client_id, video_id } => {
                                let Some(u) = &user else {
                                    if let Some(f) = err_frame("unauthorized", "登录后才能聊天") { let _ = tx.try_send(f); }
                                    continue;
                                };
                                if chat_tokens < 1.0 {
                                    if let Some(f) = err_frame("rate_limited", "消息发得太快了") { let _ = tx.try_send(f); }
                                    continue;
                                }
                                chat_tokens -= 1.0;
                                match crate::routes::chat::send_dm(&state, &state.hub, u, to, &content, video_id).await {
                                    Ok(msg) => {
                                        let ack = ServerEvent::ChatAck {
                                            id: msg.id,
                                            client_id,
                                            created_at: msg.created_at.clone(),
                                        };
                                        if let Ok(s) = serde_json::to_string(&ack) {
                                            let _ = tx.try_send(s);
                                        }
                                    }
                                    Err(e) => {
                                        if let Some(f) = err_frame("chat_failed", &e.to_string()) { let _ = tx.try_send(f); }
                                    }
                                }
                            }
                            ClientEvent::Typing { to } => {
                                if let Some(u) = &user {
                                    state.hub.send_to_user(to, &ServerEvent::Typing {
                                        from: u.id,
                                        username: u.username.clone(),
                                    });
                                }
                            }
                        }
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    // 清理
    let rooms = joined_videos.clone();
    state.hub.leave_all(conn_id);
    for vid in rooms {
        let room = format!("video:{vid}");
        let count = state.hub.presence(&room);
        state.hub.broadcast_event(
            &room,
            &ServerEvent::Presence {
                room: room.clone(),
                count,
            },
        );
    }
    writer.abort();
}

fn err_frame(code: &str, message: &str) -> Option<String> {
    serde_json::to_string(&ServerEvent::Error {
        code: code.to_string(),
        message: message.to_string(),
    })
    .ok()
}

async fn video_exists(state: &AppState, video_id: i64) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT 1 FROM videos WHERE id = ? AND status = 'ready'")
        .bind(video_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .is_some()
}

/// 通知在线用户（评论/点赞/好友等即时事件）
pub fn push_notification(hub: &Hub, user_id: i64, payload: serde_json::Value) -> bool {
    hub.send_to_user(user_id, &ServerEvent::Notification { data: payload })
}

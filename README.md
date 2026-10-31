# Light-Video · 轻视频

> 用 Rust 重写的高性能视频社区：分片上传、多分辨率、WebSocket 实时弹幕与私信、手写播放器。

旧版是 Flask + SQLAlchemy 的单体应用（已归档到 `legacy/`）。本版本在**同一套产品形态**上重做了后端与前端，
目标是「能上生产」而不是「能跑起来」：安全默认开启、失败可降级、关键路径有测试。

---

## 1. 技术栈

| 层 | 选型 | 说明 |
|---|---|---|
| Web 框架 | axum 0.8 | 基于 hyper 1.x / tower 中间件，Tokio 多线程运行时 |
| 运行时 | tokio 1.x | 每核一个 worker，异步 IO |
| 数据库 | SQLite（sqlx 0.8） | WAL 模式，编译期嵌入迁移，无 ORM 运行时开销 |
| 实时通道 | axum WebSocket | 房间广播 + 个人频道，慢客户端丢弃保护 |
| 鉴权 | JWT(HS256) + Cookie + CSRF 双提交 | Cookie 为 HttpOnly，前端拿不到令牌 |
| 密码 | Argon2id (m=19MiB, t=2, p=1) | OWASP 推荐参数 |
| 前端 | 原生 ES Modules + Canvas | 零构建、零 CDN 依赖，严格 CSP 下可运行 |
| 转码 | ffmpeg / ffprobe（可选） | 未安装时自动降级为原片直出 + Range 拖动 |

**已验证工具链**：rustc 1.98.1 / Node 22（仅用于前端语法检查与 CDP 测试）。
`Cargo.toml` 声明 `rust-version = "1.82"`，请以实际构建环境为准。

---

## 2. 架构

```
                    ┌─────────────────────────────────────────┐
  Browser ──HTTP──▶ │  axum Router                            │
   (SPA)            │  ├─ 安全中间件：CSP / CSRF / Origin      │
                    │  ├─ /api/*   REST（JSON）                │
                    │  ├─ /ws      WebSocket Hub（房间广播）   │
                    │  ├─ /media/* Range 流式（206/ETag）      │
                    │  └─ /assets/* + SPA fallback             │
                    └───────┬──────────────────┬──────────────┘
                            │                  │
                   ┌────────▼───────┐  ┌───────▼─────────┐
                   │ SQLite (WAL)   │  │ 媒体目录        │
                   │ 迁移 + 索引     │  │ media/ covers/  │
                   └────────────────┘  └───────┬─────────┘
                                               │
                                     ┌─────────▼──────────┐
                                     │ 后台任务队列        │
                                     │ ffprobe → 封面 →   │
                                     │ 多分辨率 MP4        │
                                     └────────────────────┘
```

单进程即可吃满多核：HTTP、WebSocket、磁盘 IO、转码排队都在同一 Tokio 运行时内，无跨进程通信开销。

---

## 3. 需求对照

| 目标 | 落地方式 |
|---|---|
| Rust 高性能后端 | axum + tokio + sqlx；零拷贝分片写入、流式响应、内存计数批量落库 |
| 高吞吐 / 高并发 | WAL、连接池、DashMap 广播、`try_send` 背压丢弃、gzip/br 压缩、ETag 缓存 |
| 前端更好看 | 浅色简约设计：白卡片 + 单一粉色点缀、小圆角、暗色为中性灰；骨架屏与响应式 |
| 安全性（重点） | 见第 8 节，10 项默认开启 + 审计日志 + 越权校验 |
| SQLite | WAL + `synchronous=NORMAL` + 64MB page cache + mmap，迁移随二进制编译 |
| 视频分片 | 4MB 分片、3 并发、失败重试 3 次、断点续查、24h 过期清理、服务端重新组装并校验魔数 |
| 封面 | 上传端浏览器截帧生成（无需 ffmpeg）；也可手动上传替换 |
| 更好的弹幕 | Canvas 引擎：轨道避让、滚动/顶部/底部、颜色、透明度、字号、速度、显示区域、实时 WS 下发 |
| WS 双工 | 视频房间（弹幕/在线人数）+ 个人频道（私信/通知/好友请求）+ 输入中提示 + ack |
| 手写播放器 | 自绘控制条、缓冲条、拖拽预览、音量、倍速、画质、画中画、全屏、影院模式、快捷键 |
| 弹幕框在播放器内 | 播放器底部悬浮输入条（颜色、模式、发送），随控制条自动隐藏 |
| 可改分辨率 | 有 ffmpeg 时生成 360/540/720/1080 多档 MP4，切换保留播放位置；无 ffmpeg 时只有「原画」 |
| 更多功能 | 搜索、分区、热门/关注流、观看历史、收藏、个人主页、通知、好友、私信、分享、标签、可见性 |
| 相关视频 | 没有推荐模型，直接随机抽取（`ORDER BY RANDOM()`），不做假的「相似度」 |
| 兼容性 | 无外部 CDN、MP4 + Range、移动端触控、`prefers-reduced-motion`、原生 ES Modules |
| 生产规格 | 健康检查、结构化日志、优雅退出、环境变量配置、Docker/systemd/Nginx、测试与冒烟脚本 |

---

## 4. 快速开始

### 4.1 本地运行

```bash
cd server
cp .env.example .env        # 可选，默认值即可跑
cargo run                   # 首次编译约 3-5 分钟
```

打开 <http://localhost:3000>。默认数据落在 `server/data/`。

> 想体验多分辨率：先安装 ffmpeg 并确保在 `PATH` 中，再上传视频。没有 ffmpeg 也能正常播放、拖动、发弹幕。

### 4.2 生产构建

```bash
cd server
cargo build --release       # LTO + codegen-units=1 + panic=abort
./target/release/lightvideo
```

---

## 5. 配置

全部通过环境变量（`server/.env.example` 有完整注释）：

| 变量 | 默认 | 说明 |
|---|---|---|
| `BIND_HOST` / `PORT` | `0.0.0.0` / `3000` | 监听地址 |
| `PUBLIC_URL` | `http://localhost:3000` | 分享链接与 Origin 校验基准，生产必须填真实域名 |
| `DATA_DIR` | `./data` | 数据库、媒体、临时分片根目录 |
| `DATABASE_PATH` | `$DATA_DIR/lightvideo.db` | SQLite 文件 |
| `WEB_DIR` | 自动探测 `web/` | 前端静态目录 |
| `SECURE_COOKIES` | `false` | HTTPS 下必须 `true` |
| `ALLOWED_ORIGINS` | 空 | 跨域部署时填 Origin，逗号分隔 |
| `TRUST_PROXY` | `false` | 在 Nginx/Caddy 后设 `true`，否则限流按反代 IP 统计 |
| `JWT_SECRET` | 自动生成 | 未设置时写入 `$DATA_DIR/jwt.key`（权限 600） |
| `SESSION_DAYS` | `14` | 勾选「保持登录」的有效期；不勾选为 1 天 |
| `MAX_VIDEO_MB` | `4096` | 单视频上限 |
| `MAX_COVER_MB` | `12` | 封面/头像上限 |
| `CHUNK_SIZE_MB` | `4` | 分片大小，需小于反代 `client_max_body_size` |
| `MAX_UPLOAD_SESSIONS` | `6` | 单用户并行上传会话上限 |
| `FFMPEG_PATH` / `FFPROBE_PATH` | `ffmpeg` / `ffprobe` | 不存在则自动降级 |
| `WORKER_THREADS` | 0（=全部核心） | Tokio worker 数 |

---

## 6. 目录结构

```
.
├── server/                 # Rust 后端
│   ├── Cargo.toml
│   ├── migrations/         # SQL 迁移（编译期嵌入）
│   ├── src/
│   │   ├── main.rs         # 运行时、后台任务、优雅退出
│   │   ├── lib.rs          # 路由装配（供集成测试直接驱动）
│   │   ├── config.rs       # 环境变量与目录
│   │   ├── db.rs           # 连接池 + PRAGMA + 迁移
│   │   ├── error.rs        # 统一错误 → JSON，内部错误不泄露
│   │   ├── security.rs     # 密码、JWT、Cookie、CSRF、限流、安全头
│   │   ├── auth.rs         # 会话解析缓存 + 提取器 + 审计
│   │   ├── media.rs        # Range 流式、魔数校验、ffmpeg 封装
│   │   ├── ws.rs           # WebSocket Hub 与协议
│   │   ├── models.rs       # 行结构与输出 DTO
│   │   └── routes/         # 认证/视频/上传/弹幕/评论/社交/私信/通知
│   └── tests/smoke.rs      # 端到端集成测试
├── web/                    # 前端（无构建）
│   ├── index.html
│   └── assets/
│       ├── css/app.css     # 设计系统 + 播放器样式
│       └── js/
│           ├── app.js      # 启动、顶栏、搜索
│           ├── router.js   # History 路由
│           ├── player.js   # 手写播放器
│           ├── danmaku.js  # Canvas 弹幕引擎
│           ├── ws.js       # 自动重连的 WS 客户端
│           └── pages/      # 首页/播放/投稿/主页/好友/私信/通知/登录
├── scripts/
│   ├── ui-smoke.sh            # 造数据 + 无头浏览器截图 + DOM 断言
│   ├── cdp-shot.mjs           # CDP 受控播放截图（等待真实播放进度）
│   ├── cdp-upload.mjs         # 浏览器内选文件 → 自动截帧 → 分片上传
│   ├── cdp-nav-check.mjs      # 路由切换：滚动复位 / CLS / 返回恢复
│   ├── cdp-danmaku-check.mjs  # 弹幕悬停保持、去重、切页后不重复
│   ├── cdp-pages.mjs          # 登录态页面巡检（5 个页面 + 控制台错误）
│   └── cdp-ws-check.mjs       # 登录 → WS 发弹幕 → 落库校验
├── deploy/                 # Nginx / systemd
├── Dockerfile / docker-compose.yml
└── legacy/                 # 旧 Flask 版本（存档，不再维护）
```

---

## 7. 播放器与弹幕

**播放器** `web/assets/js/player.js`（自绘，不依赖任何播放库）：

- 控制条：播放/暂停、进度（缓冲条 + 悬停时间提示 + 拖拽）、时间、音量、倍速（0.5–2x）、画质、画中画、全屏
- 影院模式：播放器吸顶铺满，隐藏侧栏，`Esc` 退出
- 自动隐藏 UI、移动端点击切换、双击全屏
- 快捷键：`空格/K` 播放暂停、`←/→` 5 秒、`↑/↓` 音量、`M` 静音、`F` 全屏、`D` 弹幕、`P` 画中画、`[` `]` 调速
- 断点续播：进入页面若上次进度在 8 秒后，提示并自动定位
- 画质切换：保留当前播放位置与播放状态；`自动` 依据播放器高度与 DPR 选档

**视觉基调**：浅灰底（`#f5f5f7`）+ 白卡片 + 单一粉色（`#fb7299`）+ 6–10px 小圆角，不使用渐变与毛玻璃；
暗色主题为中性灰，不引入第二种彩色。改主题只改 CSS 变量，组件无需改动。

**弹幕** `web/assets/js/danmaku.js`：

- Canvas 渲染，DPR 感知，最多 220 条同屏
- 轨道分配 + 尾部避让，满屏时丢弃而不是重叠
- 三种模式（滚动/顶部/底部）、自定义颜色、字号
- 可调：显示开关、不透明度、字号、速度、显示区域（25%–100% 画面高度）
- 跳转/倒退自动重建轨道索引；暂停时冻结
- 通过 WebSocket 实时接收，与 HTTP 兜底发送共用同一套校验

---

## 8. 安全设计

| 项 | 实现 |
|---|---|
| 密码存储 | Argon2id，随机盐；登录失败对不存在账号也执行一次哈希校验，降低枚举差异 |
| 会话 | JWT HS256，HttpOnly + SameSite=Lax Cookie；`token_version` 支持改密后全端失效；45 秒会话缓存兼顾吊销与吞吐 |
| CSRF | 双提交 Cookie + `X-CSRF-Token`；同时校验 `Origin` / `Sec-Fetch-Site`，双保险 |
| XSS | 前端统一 `esc()` 转义；严格 CSP（`script-src 'self'`，无内联脚本、无外部 CDN）；`nosniff` |
| 点击劫持 | `X-Frame-Options: DENY` + `frame-ancestors 'none'` |
| 传输 | 可选 HSTS；`SECURE_COOKIES=true` 时 Cookie 仅 HTTPS |
| 上传 | 扩展名白名单 **+ 文件头魔数校验**；文件名由服务端 UUID 生成；大小上限；分片序号与大小双重校验 |
| 路径穿越 / 任意文件读取 | 存储 key 字符白名单 + 仅允许 `media/`、`covers/`、`avatars/` 三类前缀 + `canonicalize` 前缀校验；数据库、`jwt.key` 等一律 404 |
| 越权 | 所有写操作校验资源归属（视频/评论/弹幕/会话），管理员例外显式判断 |
| 限流 | 登录（IP + 账号双维度）、注册、上传、弹幕、评论、私信、搜索分别限流；滑动窗口 + 周期 GC |
| WS 安全 | 握手校验 Origin；房间数上限；消息大小上限；每连接令牌桶；慢消费者丢弃 |
| 信息泄露 | 数据库/内部错误统一返回「服务器内部错误」，真实原因只进日志 |
| 审计 | 登录成功/失败、封禁、改密、上传、头像更新写入 `audit_log`，保留 30 天 |

> 说明：XSS 防护依赖「所有动态文本走 `esc()`」这一约定，已在全部页面执行；若后续引入富文本，需要接 HTML 白名单过滤（如 ammonia）。

---

## 9. 性能设计

1. **SQLite**：`journal_mode=WAL`、`synchronous=NORMAL`、64MiB page cache、256MiB mmap、`busy_timeout=5s`，读不阻塞写。
2. **连接池**：`2×CPU`（8–32），避免过度连接导致锁竞争。
3. **播放量**：内存累加，5 秒批量 `UPDATE`，同一客户端 30 分钟只计一次，避免热点行写放大。
4. **上传**：分片直写磁盘（临时文件 + 原子 rename），组装用 1MiB `BufWriter` 流式拷贝，全程不把整段视频读进内存。
5. **媒体响应**：`ReaderStream` 128KiB 分块 + `Range`/`ETag`/`If-None-Match`，拖动不重传整文件。
6. **广播**：`DashMap` 房间表 + `mpsc` 有界队列 + `try_send`，慢客户端丢帧而不拖垮进程。
7. **压缩**：仅对 API 响应启用 br/gzip；媒体文件不压缩。
8. **前端**：零依赖、原生 ESM、图片 `loading=lazy`、骨架屏。
9. **线程**：`worker_threads = CPU 核数`，`panic=abort` + LTO 减小体积与分支开销。

---

## 10. 视频处理

```
init → 分片 PUT（并发 3，重试 3） → complete（组装 + 魔数校验 + 入库）
                                          │
                        ┌─────────────────┴──────────────────┐
                        │ ffmpeg 可用                        │ 不可用
                        ▼                                    ▼
         ffprobe 时长/分辨率 → 抽帧封面              status=ready
         → 360/540/720/1080 多档 MP4（faststart）    只有「原画」
         → status=ready，前端出现画质菜单            进度条/Range 正常
```

- **封面**：未手动选择时，上传端用浏览器解码视频并截取 1.2s 处一帧，压缩成 JPEG 后上传（`web/assets/js/thumb.js`）。
  这一步只在上传者本机执行一次，访客不会重复解码，也不依赖 ffmpeg；截帧失败才退回灰底占位块。
- 分片会话 24 小时过期，后台每 30 分钟清理，避免磁盘泄漏。
- 组装前逐片校验存在与总大小，缺失直接提示重新上传。
- 转码失败不影响发布：`status` 兜底为 `ready`，原片仍可播放。

**为什么不用 HLS？**
浏览器对 HLS 的原生支持仅 Safari/iOS 完整，Chrome/Firefox 需要 MSE 播放器（hls.js）或自研 TS/fMP4 解复用。
在严格 CSP 且不引入第三方库的前提下，多分辨率 MP4 + Range 的兼容性更好、实现更可靠，拖动响应也更快。
若后续确需 HLS（超长视频、弱网自适应），建议引入 hls.js 或服务端 fMP4 切片并单独评估，而不是在播放器里手写解复用。

---

## 11. API 概览

所有响应统一为 `{ "ok": true, "data": ... }` 或 `{ "error": { "code", "message" } }`。
写操作必须携带 `X-CSRF-Token`（值等于 `lv_csrf` Cookie）。

```
POST   /api/auth/register            注册（返回会话 Cookie）
POST   /api/auth/login               登录
POST   /api/auth/logout              登出
GET    /api/auth/me                  当前用户 + 未读数
PATCH  /api/auth/profile             昵称/简介
POST   /api/auth/avatar              头像（multipart）
POST   /api/auth/password            改密（使旧会话全部失效）

GET    /api/videos                   列表 ?feed=latest|hot|following&category=&q=&page=
GET    /api/videos/{id}              详情（含 liked/favorited/renditions）
PATCH  /api/videos/{id}              改标题/简介/分区/标签/可见性
DELETE /api/videos/{id}              删除（同时清理磁盘）
GET    /api/videos/{id}/related      相关推荐
GET    /api/videos/{id}/progress     续播进度
POST   /api/videos/{id}/progress     上报进度
GET    /api/me/favorites             我的收藏
GET    /api/me/history               观看历史

POST   /api/uploads/init             创建分片会话
PUT    /api/uploads/{id}/chunk/{idx} 上传分片（原始字节）
GET    /api/uploads/{id}             查询已收分片
DELETE /api/uploads/{id}             取消
POST   /api/uploads/{id}/complete    组装 + 建视频
POST   /api/uploads/cover            封面上传

GET    /api/videos/{id}/danmaku      弹幕列表（可按时间区间）
POST   /api/videos/{id}/danmaku      发送弹幕（WS 不可用时兜底）
DELETE /api/danmaku/{id}             删除（作者/UP主/管理员）

GET    /api/videos/{id}/comments     评论分页
POST   /api/videos/{id}/comments     发表评论
DELETE /api/comments/{id}            删除评论

POST   /api/videos/{id}/like         点赞/取消
POST   /api/videos/{id}/favorite     收藏/取消
POST   /api/videos/{id}/share        分享计数
GET    /api/friends                  好友列表（含在线状态）
GET    /api/friends/requests         待处理请求
POST   /api/users/{id}/friend-request|friend-accept|friend-reject
DELETE /api/users/{id}/friend        删除好友
GET    /api/users/search?q=          搜索用户
GET    /api/users/{id}               公开主页

GET    /api/conversations            会话列表
GET    /api/messages/{userId}        历史消息（自动已读）
POST   /api/messages                 发私信（HTTP 兜底）
POST   /api/messages/read            标记已读
GET    /api/notifications            通知列表
POST   /api/notifications/read       全部已读
GET    /api/health                   健康检查
GET    /api/stats                    站点统计
```

**WebSocket** `/ws`：连接即加入个人频道，可 `join` 视频房间。

```jsonc
// 客户端 → 服务端
{"type":"join","video_id":12}
{"type":"danmaku","video_id":12,"content":"前方高能","time_ms":4200,"color":"#ff5c8a","mode":"scroll","font_size":25}
{"type":"chat","to":7,"content":"在吗","client_id":"c-1"}
{"type":"typing","to":7}
{"type":"ping","t":1730000000000}

// 服务端 → 客户端
{"type":"hello","user":{"id":1,"username":"demo"},"server_time":1730000000000}
{"type":"danmaku","data":{}}
{"type":"chat","data":{}}
{"type":"chat_ack","id":42,"client_id":"c-1","created_at":"..."}
{"type":"presence","room":"video:12","count":3}
{"type":"notification","data":{}}
{"type":"error","code":"rate_limited","message":"弹幕发得太快了"}
```

---

## 12. 数据库

13 张表：`users / videos / video_renditions / uploads / likes / favorites / comments /
danmaku / friendships / messages / notifications / watch_history / audit_log`。

要点：

- 时间戳统一 Unix 毫秒（`INTEGER`），跨时区排序稳定。
- 计数列（`likes_count` 等）与关联表同事务更新，列表页无需 `COUNT(*)`。
- 全部外键 `ON DELETE CASCADE`，删除视频自动清理关联数据；磁盘文件异步删除。
- 迁移内嵌在二进制中，启动时自动执行，升级只需替换可执行文件。

备份：

```bash
sqlite3 data/lightvideo.db ".backup '/backup/lv-$(date +%F).db'"
rsync -a data/media data/covers /backup/media/
```

---

## 13. 测试与验证

### 13.1 集成测试（真实 Router + 真实 SQLite）

```bash
cd server && cargo test -- --nocapture
```

实测结果：

```
running 3 tests
test full_api_flow ... ok
test login_and_rate_limit ... ok
test ws_hub_broadcast ... ok
test result: ok. 3 passed; 0 failed
```

覆盖：健康检查、CSRF 拒绝、注册/重复注册/弱密码、会话、未登录 401、
分片上传全流程、列表/详情、Range 206、路径穿越与敏感文件读取拦截、弹幕颜色与字号清洗、
评论、点赞、收藏列表、越权删除视频/评论、非法分区、登录暴力破解限流、WS 广播与在线数。

### 13.2 UI 冒烟（无头 Chromium）

```bash
bash scripts/ui-smoke.sh 3011 ./data-demo
```

脚本会：起服务 → 注册 → 生成低饱和封面与 MP4 魔数桩 → 上传 6 个视频 → 写入弹幕/评论/点赞
→ 截图首页/播放页/移动端 → DOM 断言。

实测输出：

```
DOM: SPA mounted
DOM: cards rendered
DOM: player mounted
DOM: comments rendered
```

### 13.3 真实播放与弹幕（CDP）

```bash
node scripts/cdp-shot.mjs 9333 "http://127.0.0.1:3000/watch/7?autoplay=1" shot.png 2.2
```

实测：`{"t":2.21,"paused":false,"ready":4}`，截图可见滚动/顶部/底部三种弹幕叠加在视频画面上，控制台零报错。

### 13.4 WebSocket 全链路

```bash
node scripts/cdp-ws-check.mjs 9334 "http://127.0.0.1:3000" 7 ws.png
```

实测：`login=200`、`connected=true`、弹幕数 `4 → 5`、内容已落库、视频播放到 `3.46s`、零控制台错误。

### 13.5 交互回归（悬停 / 去重 / 切页）

```bash
node scripts/cdp-danmaku-check.mjs 9345 "http://127.0.0.1:3000" 1 dm.png
```

实测：

```
hover(4s)   → hideUi:false, barOpacity:1, input 可见
发一条      → 服务端 +1，列表 1 行，spawnDelta=1, activeSame=1, itemsSame=1
快速切页×3  → 再发一条仍 +1
控制台错误  → 0
```

### 13.6 路由切换（滚动 / 布局位移）

```bash
node scripts/cdp-nav-check.mjs 9342 "http://127.0.0.1:3000" nav.png
```

实测：点击后 `scrollY 210 → 0`（60ms 内），进度条 `route-progress on`，CLS `0.0083`，
返回后恢复 `210.67`，零 console 错误。

---

## 14. 生产部署

### 14.1 Docker

```bash
cp server/.env.example .env    # 按需修改 PUBLIC_URL / JWT_SECRET
docker compose up -d --build
docker compose logs -f lightvideo
```

- 容器以非 root（uid 10001）运行，`read_only` 根文件系统，仅 `/data` 可写。
- 需要 TLS 时启用附带 Nginx：`docker compose --profile tls up -d`（证书放 `data/certs`）。

### 14.2 systemd

```bash
sudo useradd -r -s /usr/sbin/nologin lightvideo
sudo mkdir -p /opt/lightvideo/bin /var/lib/lightvideo /opt/lightvideo/tmp
sudo cp server/target/release/lightvideo /opt/lightvideo/bin/
sudo cp -r web /opt/lightvideo/
sudo cp deploy/light-video.service /etc/systemd/system/
sudo install -d /etc/lightvideo && sudo cp server/.env.example /etc/lightvideo/env
sudo systemctl enable --now light-video
```

`EnvironmentFile` 中至少设置：

```
DATA_DIR=/var/lib/lightvideo
WEB_DIR=/opt/lightvideo/web
PUBLIC_URL=https://video.example.com
SECURE_COOKIES=true
TRUST_PROXY=true
```

### 14.3 Nginx

见 `deploy/nginx.conf`（HTTPS、WS 升级、Range 透传、`client_max_body_size 24m`）。

**注意**：`client_max_body_size` 必须大于分片大小（默认 4MB），但不要设成视频总大小，
否则会绕过应用层分片逻辑。

### 14.4 上线检查清单

- [ ] `PUBLIC_URL` 为真实 https 域名
- [ ] `SECURE_COOKIES=true`，`TRUST_PROXY=true`
- [ ] `JWT_SECRET` 独立且 ≥ 32 字节（或确认 `jwt.key` 已持久化）
- [ ] `/data` 有独立磁盘与备份策略
- [ ] 反代放行 `Upgrade`，`proxy_read_timeout` ≥ 300s
- [ ] 限制 `/api/auth/*` 的来源或加 WAF（应用内已有双维度限流）
- [ ] 监控 `/api/health` 与磁盘余量

---

## 15. 已知限制与演进

诚实列出边界，避免误用：

1. **单实例**：WebSocket Hub 是进程内 `DashMap`，多副本部署时不同实例的用户收不到彼此消息。
   需要水平扩展时，把 `ws::Hub` 的 `broadcast/send_to_user` 换成 Redis Pub/Sub 或 NATS 即可，接口已隔离。
2. **SQLite 写串行**：适合单机、中小规模（万级日活、写 QPS 数百）。写热点已做批量聚合；
   若写 QPS 持续过千，建议迁移 PostgreSQL（`sqlx` 换 driver + 方言迁移，业务 SQL 基本可复用）。
3. **无 HLS**：理由见第 10 节，这是有意的取舍，不是未完成项。
4. **未迁移的功能**：旧版「凌界 OAuth2 第三方登录」未移植（原实现把 `client_secret` 硬编码在源码里，属于必须修掉的问题）。
   如需保留，应改为标准 OAuth2 授权码流程 + PKCE，密钥走环境变量。
5. **无邮箱验证/找回密码**：当前依赖用户名 + 密码；接入邮件服务后可补。
6. **无管理后台**：`role=admin` 已有字段与权限判断，但没有 UI；可通过 API 或直接改库。
7. **前端无 SSR**：纯 SPA，首屏依赖 JS；对 SEO 有要求时需要预渲染或换框架。

---

## 16. 从 Flask 迁移

- 旧代码完整保留在 `legacy/`，不再维护，也不参与构建。
- 数据库结构不兼容（时间戳、计数列、friendship 模型均重做），旧 SQLite 数据需要写脚本迁移。
- API 从「表单 + 服务端模板」改为「JSON + SPA」，Cookie 名由 Flask session 改为 `lv_session`。
- 旧版的明文 `secret_key`、硬编码 OAuth 密钥、无 CSRF、无上传魔数校验等问题已全部移除。

---

## 17. License

MIT，见 `LICENSE`。

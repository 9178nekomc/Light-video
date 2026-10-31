#!/usr/bin/env bash
# UI 冒烟：起服务 → 造数据 → 无头 Chromium 截图 + DOM 断言
# 用法: bash scripts/ui-smoke.sh [port] [datadir]
set -euo pipefail

PORT="${1:-3011}"
DATA_DIR="${2:-./data-demo}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SERVER_DIR="$ROOT/server"
OUT_DIR="$ROOT/.smoke"
CHROME="${CHROME_PATH:-$USERPROFILE/AppData/Local/ms-playwright/chromium-1091/chrome-win/chrome.exe}"
BIN="${BIN:-./target/debug/lightvideo.exe}"
[ -f "$BIN" ] || BIN="./target/debug/lightvideo"

mkdir -p "$OUT_DIR"
cd "$SERVER_DIR"
rm -rf "$DATA_DIR"
mkdir -p "$DATA_DIR"

PORT=$PORT BIND_HOST=127.0.0.1 WEB_DIR=../web DATA_DIR="$DATA_DIR" \
  PUBLIC_URL="http://127.0.0.1:$PORT" "$BIN" > "$OUT_DIR/server.log" 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null || true' EXIT

for _ in $(seq 1 40); do
  if curl -sf "http://127.0.0.1:$PORT/api/health" > /dev/null; then break; fi
  sleep 0.25
done
echo "server: $(curl -s "http://127.0.0.1:$PORT/api/health")"

# ---------- 生成测试素材（纯 Python，避免二进制进 shell） ----------
python - "$OUT_DIR" <<'PY'
import sys, zlib, struct, os
out = sys.argv[1]
os.makedirs(out, exist_ok=True)

def png(path, top, bottom, w=640, h=360):
    rows = bytearray()
    for y in range(h):
        t = y / (h - 1)
        r = int(top[0] + (bottom[0] - top[0]) * t)
        g = int(top[1] + (bottom[1] - top[1]) * t)
        b = int(top[2] + (bottom[2] - top[2]) * t)
        rows += b'\x00' + bytes([r, g, b]) * w
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
    ihdr = struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)
    data = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', ihdr)
            + chunk(b'IDAT', zlib.compress(bytes(rows), 9)) + chunk(b'IEND', b''))
    open(path, 'wb').write(data)

# 克制的低饱和色，避免测试图本身抢戏
palettes = [((251, 114, 153), (214, 132, 154)), ((120, 132, 148), (86, 96, 110)),
            ((201, 151, 77), (176, 137, 84)), ((92, 156, 134), (74, 126, 110)),
            ((161, 122, 141), (133, 104, 120)), ((108, 126, 158), (88, 104, 132))]
for i, (a, b) in enumerate(palettes, 1):
    png(f'{out}/cover{i}.png', a, b)

# 最小可被魔数识别的 MP4 头（真实播放需真视频，这里只用于建数据）
stub = struct.pack('>I', 32) + b'ftypisom' + b'\x00\x00\x02\x00isomiso2avc1mp41' + b'\x00' * 2048
open(f'{out}/stub.mp4', 'wb').write(stub)
print('fixtures ok')
PY

JAR="$OUT_DIR/cookies.txt"
rm -f "$JAR"
curl -s -c "$JAR" -o /dev/null "http://127.0.0.1:$PORT/"
CSRF=$(awk '/lv_csrf/ {print $7}' "$JAR")

api() { # method path json [output]
  local out="${4:-/dev/null}"
  curl -s -b "$JAR" -c "$JAR" -X "$1" "http://127.0.0.1:$PORT$2" \
    -H 'Content-Type: application/json' -H "X-CSRF-Token: $CSRF" -d "$3" -o "$out" -w '%{http_code}'
}

code=$(api POST /api/auth/register '{"username":"demo","email":"demo@example.com","password":"Demo12345"}' "$OUT_DIR/reg.json")
[ "$code" = "200" ] || { echo "register failed: $code $(cat "$OUT_DIR/reg.json")"; exit 1; }
CSRF=$(awk '/lv_csrf/ {print $7}' "$JAR")
echo "user: $(curl -s -b "$JAR" "http://127.0.0.1:$PORT/api/auth/me" | python -c 'import sys,json;d=json.load(sys.stdin)["data"];print(d["user"]["username"], d["stats"])')"

TITLES=("用 Rust 重写后端：从 Flask 到 Axum 的完整迁移" "手写播放器与 Canvas 弹幕引擎拆解" "SQLite 在高并发下到底能扛多少" "自学第三年：我的开源项目复盘" "把视频分片上传做到生产级" "WebSocket 双工私信的设计取舍")
CATS=(tech tech tech life tech tech)
TAGS=("Rust,后端,重构" "播放器,弹幕,Canvas" "SQLite,性能,并发" "开源,成长,复盘" "分片上传,HTTP,工程" "WebSocket,实时,架构")
SIZE=$(wc -c < "$OUT_DIR/stub.mp4")

for i in 0 1 2 3 4 5; do
  api POST /api/uploads/init "{\"filename\":\"demo$i.mp4\",\"total_size\":$SIZE}" "$OUT_DIR/init.json" > /dev/null
  UID_=$(python -c 'import json,sys;print(json.load(open(sys.argv[1]))["data"]["upload_id"])' "$OUT_DIR/init.json")
  curl -s -b "$JAR" -c "$JAR" -X PUT "http://127.0.0.1:$PORT/api/uploads/$UID_/chunk/0" \
    -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/octet-stream' \
    --data-binary "@$OUT_DIR/stub.mp4" -o /dev/null
  COVER=$(curl -s -b "$JAR" -c "$JAR" -X POST "http://127.0.0.1:$PORT/api/uploads/cover" \
    -H "X-CSRF-Token: $CSRF" -F "cover=@$OUT_DIR/cover$((i+1)).png" \
    | python -c 'import json,sys;print(json.load(sys.stdin)["data"]["cover_key"])')
  META=$(python - "$i" "${TITLES[$i]}" "${CATS[$i]}" "${TAGS[$i]}" "$COVER" <<'PY'
import json, sys
i, title, cat, tags, cover = sys.argv[1:6]
print(json.dumps({
    "title": title,
    "description": "示例视频，用于验证播放器、弹幕与评论区布局。\n\n内容由脚本生成，仅作 UI 冒烟。",
    "category": cat,
    "tags": tags.split(','),
    "cover_key": cover,
}, ensure_ascii=False))
PY
)
  code=$(api POST "/api/uploads/$UID_/complete" "$META" "$OUT_DIR/complete.json")
  [ "$code" = "200" ] || { echo "complete failed: $code $(cat "$OUT_DIR/complete.json")"; exit 1; }
  echo "seeded $((i+1))"
done

FIRST=$(curl -s "http://127.0.0.1:$PORT/api/videos?limit=1" | python -c 'import json,sys;print(json.load(sys.stdin)["data"]["items"][0]["id"])')
echo "first video id: $FIRST"

for d in '{"content":"这个控制条是手写的？有点东西","time_ms":900,"color":"#ff5c8a","mode":"scroll"}' \
         '{"content":"弹幕避让做得不错","time_ms":1800,"color":"#ffffff","mode":"scroll"}' \
         '{"content":"Rust 后端确实稳","time_ms":2600,"color":"#7ee8d8","mode":"top"}' \
         '{"content":"已收藏，等更新","time_ms":3400,"color":"#ffd166","mode":"scroll"}' \
         '{"content":"底部固定弹幕也支持","time_ms":4200,"color":"#c084fc","mode":"bottom"}'; do
  api POST "/api/videos/$FIRST/danmaku" "$d" > /dev/null
done
api POST "/api/videos/$FIRST/comments" '{"content":"分片上传 + 双提交 CSRF，这套组合很实用。"}' > /dev/null
api POST "/api/videos/$FIRST/comments" '{"content":"界面比原来好看太多了，暗色很舒服。"}' > /dev/null
api POST "/api/videos/$FIRST/like" '{}' > /dev/null
api POST "/api/videos/$FIRST/favorite" '{}' > /dev/null

shot() {
  "$CHROME" --headless=new --disable-gpu --no-sandbox --hide-scrollbars \
    --force-device-scale-factor=1 --window-size="${3:-1440,1000}" \
    --virtual-time-budget="${4:-6000}" --screenshot="$OUT_DIR/$1.png" "$2" > /dev/null 2>&1 || true
}

shot home "http://127.0.0.1:$PORT/"
shot watch "http://127.0.0.1:$PORT/watch/$FIRST"
shot mobile "http://127.0.0.1:$PORT/" "420,900"

"$CHROME" --headless=new --disable-gpu --no-sandbox --virtual-time-budget=6000 \
  --dump-dom "http://127.0.0.1:$PORT/" > "$OUT_DIR/home.html" 2>/dev/null || true
"$CHROME" --headless=new --disable-gpu --no-sandbox --virtual-time-budget=6000 \
  --dump-dom "http://127.0.0.1:$PORT/watch/$FIRST" > "$OUT_DIR/watch.html" 2>/dev/null || true

# ---------- 真实视频（若已下载）验证解码与弹幕渲染 ----------
if [ -f "$OUT_DIR/flower.webm" ]; then
  REAL_SIZE=$(wc -c < "$OUT_DIR/flower.webm")
  api POST /api/uploads/init "{\"filename\":\"flower.webm\",\"total_size\":$REAL_SIZE}" "$OUT_DIR/init2.json" > /dev/null
  RID=$(python -c 'import json,sys;print(json.load(open(sys.argv[1]))["data"]["upload_id"])' "$OUT_DIR/init2.json")
  curl -s -b "$JAR" -c "$JAR" -X PUT "http://127.0.0.1:$PORT/api/uploads/$RID/chunk/0"     -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/octet-stream'     --data-binary "@$OUT_DIR/flower.webm" -o /dev/null
  COVER7=$(curl -s -b "$JAR" -c "$JAR" -X POST "http://127.0.0.1:$PORT/api/uploads/cover"     -H "X-CSRF-Token: $CSRF" -F "cover=@$OUT_DIR/cover1.png"     | python -c 'import json,sys;print(json.load(sys.stdin)["data"]["cover_key"])')
  api POST "/api/uploads/$RID/complete" "{\"title\":\"真实视频：播放器与弹幕实拍\",\"description\":\"用于验证真实解码、进度条与弹幕层。\",\"category\":\"tech\",\"tags\":[\"demo\"],\"cover_key\":\"$COVER7\"}" "$OUT_DIR/c2.json" > /dev/null
  REAL=$(python -c 'import json,sys;print(json.load(open(sys.argv[1]))["data"]["video_id"])' "$OUT_DIR/c2.json")
  for d in '{"content":"真实视频也能跑","time_ms":400,"color":"#ffffff","mode":"scroll"}'            '{"content":"弹幕在视频上方渲染","time_ms":900,"color":"#ff5c8a","mode":"scroll"}'            '{"content":"顶部弹幕","time_ms":1400,"color":"#7ee8d8","mode":"top"}'            '{"content":"底部的也可以","time_ms":1900,"color":"#ffd166","mode":"bottom"}'; do
    api POST "/api/videos/$REAL/danmaku" "$d" > /dev/null
  done
  echo "real video id: $REAL"
  # 静态截图模式下无头 Chrome 的虚拟时间不会推进媒体播放，
  # 真实播放帧请用 scripts/cdp-shot.mjs 验证
  shot watch-real "http://127.0.0.1:$PORT/watch/$REAL" "1440,1000" "3000"
fi

grep -q "把喜欢的内容" "$OUT_DIR/home.html" && echo "DOM: SPA mounted" || echo "DOM: SPA NOT mounted"
grep -q "vcard" "$OUT_DIR/home.html" && echo "DOM: cards rendered" || echo "DOM: no cards"
grep -q "dm-canvas" "$OUT_DIR/watch.html" && echo "DOM: player mounted" || echo "DOM: player missing"
grep -q "comment" "$OUT_DIR/watch.html" && echo "DOM: comments rendered" || echo "DOM: no comments"

ls -la "$OUT_DIR"/*.png 2>/dev/null
echo done

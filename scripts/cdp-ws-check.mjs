// WebSocket 端到端校验：登录 → 进入播放页 → 通过 WS 发弹幕 → 校验落库与广播
// 用法: node scripts/cdp-ws-check.mjs <debugPort> <baseUrl> <videoId> <out.png>
import { writeFileSync } from 'node:fs';

const [, , portArg, base, videoId, out] = process.argv;
const port = Number(portArg || 9222);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const ws = new WebSocket(version.webSocketDebuggerUrl);
const pending = new Map();
const logs = [];
let seq = 0;

ws.addEventListener('message', (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
    return;
  }
  if (msg.method === 'Runtime.exceptionThrown') {
    logs.push(`[exception] ${msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text}`);
  }
  if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') {
    logs.push(`[console.error] ${msg.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
  }
});
await new Promise((resolve) => ws.addEventListener('open', resolve, { once: true }));

function send(method, params = {}, sessionId) {
  const id = ++seq;
  return new Promise((resolve) => {
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
}

const { result: { targetId } } = await send('Target.createTarget', { url: 'about:blank' });
const { result: { sessionId } } = await send('Target.attachToTarget', { targetId, flatten: true });
await send('Page.enable', {}, sessionId);
await send('Runtime.enable', {}, sessionId);
await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false }, sessionId);

const evaluate = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId);
  if (r.result?.exceptionDetails) {
    throw new Error(r.result.exceptionDetails.exception?.description || 'evaluate failed');
  }
  return r.result?.result?.value;
};

// 1. 登录（走真实接口 + CSRF 双提交）
await send('Page.navigate', { url: `${base}/login` }, sessionId);
await sleep(900);
const login = await evaluate(`(async () => {
  const csrf = (document.cookie.match(/(?:^|; )lv_csrf=([^;]*)/) || [])[1] || '';
  const res = await fetch('/api/auth/login', {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': decodeURIComponent(csrf) },
    body: JSON.stringify({ username: 'demo', password: 'Demo12345', remember: true }),
  });
  return { status: res.status, body: await res.json() };
})()`);

// 2. 进入播放页并等待 WS 就绪
await send('Page.navigate', { url: `${base}/watch/${videoId}?autoplay=1` }, sessionId);
await sleep(2200);
const before = await evaluate(`(async () => {
  const r = await fetch('/api/videos/${videoId}/danmaku');
  const j = await r.json();
  return (j.data || []).length;
})()`);

// 3. 通过页面内的 WS 发送弹幕
const sendResult = await evaluate(`(async () => {
  const m = await import('/assets/js/ws.js');
  const connected = m.ws.connected;
  m.ws.danmaku({ video_id: ${videoId}, content: 'WS 实时弹幕：双向通道打通', time_ms: 1800, color: '#34e2c4', mode: 'scroll' });
  await new Promise((r) => setTimeout(r, 1200));
  const res = await fetch('/api/videos/${videoId}/danmaku');
  const j = await res.json();
  const list = j.data || [];
  return { connected, count: list.length, found: list.some((d) => d.content.includes('WS 实时弹幕')) };
})()`);

const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
if (shot.result?.data) writeFileSync(out, Buffer.from(shot.result.data, 'base64'));

const videoState = await evaluate("(()=>{const v=document.querySelector('video');return v?{t:+v.currentTime.toFixed(2),paused:v.paused}:null;})()");

console.log(JSON.stringify({ login: login.status, before, ...sendResult, videoState, logs }, null, 2));

await send('Target.closeTarget', { targetId });
ws.close();

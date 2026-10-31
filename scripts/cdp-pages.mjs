// 登录态页面巡检：登录后逐页截图并收集控制台错误
// 用法: node scripts/cdp-pages.mjs <debugPort> <baseUrl> <outDir>
import { writeFileSync, mkdirSync } from 'node:fs';

const [, , portArg, base, outDir] = process.argv;
const port = Number(portArg || 9222);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(outDir, { recursive: true });

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
await new Promise((r) => ws.addEventListener('open', r, { once: true }));

const send = (method, params = {}, sessionId) => {
  const id = ++seq;
  return new Promise((resolve) => {
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
};

const { result: { targetId } } = await send('Target.createTarget', { url: 'about:blank' });
const { result: { sessionId } } = await send('Target.attachToTarget', { targetId, flatten: true });
await send('Page.enable', {}, sessionId);
await send('Runtime.enable', {}, sessionId);
await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1100, deviceScaleFactor: 1, mobile: false }, sessionId);

const evaluate = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId);
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'eval failed');
  return r.result?.result?.value;
};

await send('Page.navigate', { url: `${base}/login` }, sessionId);
await sleep(700);
const login = await evaluate(`(async () => {
  const csrf = (document.cookie.match(/(?:^|; )lv_csrf=([^;]*)/) || [])[1] || '';
  const res = await fetch('/api/auth/login', {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': decodeURIComponent(csrf) },
    body: JSON.stringify({ username: 'demo', password: 'Demo12345', remember: true }),
  });
  return res.status;
})()`);

const pages = [
  ['profile', '/user/1'],
  ['friends', '/friends'],
  ['chat', '/chat/2'],
  ['notifications', '/notifications'],
  ['upload', '/upload'],
];

const results = {};
for (const [name, path] of pages) {
  await send('Page.navigate', { url: `${base}${path}` }, sessionId);
  await sleep(1400);
  const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
  if (shot.result?.data) writeFileSync(`${outDir}/${name}.png`, Buffer.from(shot.result.data, 'base64'));
  results[name] = await evaluate(`(() => {
    const main = document.getElementById('view');
    const rect = (sel) => { const el = document.querySelector(sel); if (!el) return null; const r = el.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) }; };
    return {
      len: main ? main.innerText.trim().length : 0,
      empty: !!main?.querySelector('.empty'),
      title: document.title,
      rects: { side: rect('.chat-side'), main: rect('.chat-main'), head: rect('.chat-head'), bubble: rect('.msg .bubble'), card: rect('.list-card'), hero: rect('.profile-hero') },
    };
  })()`);
}

console.log(JSON.stringify({ login, results, logs: logs.slice(0, 12) }, null, 2));
await send('Target.closeTarget', { targetId });
ws.close();

// 端到端验证：浏览器内选文件 → 自动截帧封面 → 分片上传 → 跳转播放页
// 用法: node scripts/cdp-upload.mjs <debugPort> <baseUrl> <videoPath> <out.png>
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const [, , portArg, base, videoPath, out] = process.argv;
const port = Number(portArg || 9222);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const ws = new WebSocket(version.webSocketDebuggerUrl);
const pending = new Map();
const logs = [];
let seq = 0;

ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
    return;
  }
  if (m.method === 'Runtime.exceptionThrown') {
    logs.push(`[exception] ${m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text}`);
  }
  if (m.method === 'Runtime.consoleAPICalled' && m.params.type === 'error') {
    logs.push(`[console.error] ${m.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
  }
});
await new Promise((r) => ws.addEventListener('open', r, { once: true }));

const send = (method, params = {}, sessionId) => {
  const id = ++seq;
  return new Promise((res) => {
    pending.set(id, res);
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
};

const { result: { targetId } } = await send('Target.createTarget', { url: 'about:blank' });
const { result: { sessionId } } = await send('Target.attachToTarget', { targetId, flatten: true });
await send('Page.enable', {}, sessionId);
await send('Runtime.enable', {}, sessionId);
await send('DOM.enable', {}, sessionId);
await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1100, deviceScaleFactor: 1, mobile: false }, sessionId);

const evaluate = async (expression, byValue = true) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: byValue }, sessionId);
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'eval failed');
  return byValue ? r.result?.result?.value : r.result?.result;
};

// 登录
await send('Page.navigate', { url: `${base}/login` }, sessionId);
await sleep(800);
const login = await evaluate(`(async () => {
  const csrf = (document.cookie.match(/(?:^|; )lv_csrf=([^;]*)/) || [])[1] || '';
  const r = await fetch('/api/auth/login', {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': decodeURIComponent(csrf) },
    body: JSON.stringify({ username: 'demo', password: 'Demo12345', remember: true }),
  });
  return r.status;
})()`);

// 打开发布页并注入文件
await send('Page.navigate', { url: `${base}/upload` }, sessionId);
await sleep(1200);
const inputObj = await evaluate("document.getElementById('file')", false);
const objectId = inputObj?.objectId;
if (!objectId) throw new Error('file input not found');
const set = await send('DOM.setFileInputFiles', { files: [resolve(videoPath)], objectId }, sessionId);
if (set.error) throw new Error(JSON.stringify(set.error));
await evaluate("document.getElementById('file').dispatchEvent(new Event('change', { bubbles: true }))");

// 等自动封面
let cover = null;
for (let i = 0; i < 60; i += 1) {
  cover = await evaluate(`(() => {
    const img = document.querySelector('#cover-preview img');
    const hint = document.getElementById('cover-hint');
    return { hasImg: !!img, src: img?.getAttribute('src') || '', hint: hint?.textContent || '' };
  })()`);
  if (cover.hasImg) break;
  await sleep(200);
}

// 填标题并发布
await evaluate(`(() => {
  const t = document.getElementById('m-title');
  t.value = '浏览器截帧测试';
  t.dispatchEvent(new Event('input', { bubbles: true }));
  document.getElementById('meta-form').requestSubmit();
})()`);

let videoId = null;
for (let i = 0; i < 100; i += 1) {
  const href = await evaluate('location.pathname');
  if (href && href.startsWith('/watch/')) { videoId = Number(href.split('/')[2]); break; }
  await sleep(200);
}

const poster = videoId ? await evaluate(`(async () => {
  const r = await fetch('/api/videos/${videoId}');
  const j = await r.json();
  return { poster: j.data?.poster_url || '', title: j.data?.title || '' };
})()`) : null;

await send('Page.navigate', { url: `${base}/` }, sessionId);
await sleep(1600);
const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
if (shot.result?.data) writeFileSync(out, Buffer.from(shot.result.data, 'base64'));

console.log(JSON.stringify({ login, cover, videoId, poster, logs: logs.slice(0, 8) }, null, 2));
await send('Target.closeTarget', { targetId });
ws.close();

// 弹幕交互体检：悬停不收起、一条只发一次、快速切页后不重复
// 用法: node scripts/cdp-danmaku-check.mjs <debugPort> <baseUrl> <videoId> <out.png>
import { writeFileSync } from 'node:fs';

const [, , portArg, base, videoIdArg, out] = process.argv;
const port = Number(portArg || 9222);
const videoId = Number(videoIdArg);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const ws = new WebSocket(version.webSocketDebuggerUrl);
const pending = new Map();
const logs = [];
let seq = 0;

ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
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
await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false }, sessionId);

const ev = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId);
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'eval failed');
  return r.result?.result?.value;
};

// 登录
await send('Page.navigate', { url: `${base}/login` }, sessionId);
await sleep(800);
const login = await ev(`(async () => {
  const csrf = (document.cookie.match(/(?:^|; )lv_csrf=([^;]*)/) || [])[1] || '';
  const r = await fetch('/api/auth/login', {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': decodeURIComponent(csrf) },
    body: JSON.stringify({ username: 'demo', password: 'Demo12345', remember: true }),
  });
  return r.status;
})()`);

const countApi = async () => ev(`(async () => {
  const r = await fetch('/api/videos/${videoId}/danmaku');
  const j = await r.json();
  return (j.data || []).length;
})()`);

// 1) 进入播放页，鼠标停到弹幕栏上 4 秒（超过 2.4s 自动隐藏阈值）
await send('Page.navigate', { url: `${base}/watch/${videoId}?autoplay=1` }, sessionId);
await sleep(1500);
const rect = await ev(`(() => { const r = document.querySelector('.dm-bar').getBoundingClientRect(); return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) }; })()`);
await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: rect.x, y: rect.y, button: 'none' }, sessionId);
await sleep(4000);
const hoverState = await ev(`(() => {
  const p = document.querySelector('.player');
  const bar = document.querySelector('.dm-bar');
  const input = document.querySelector('.dm-input');
  return {
    hideUi: p.classList.contains('hide-ui'),
    barOpacity: getComputedStyle(bar).opacity,
    inputVisible: input.getBoundingClientRect().height > 0,
  };
})()`);

// 2) 发一条，检查只 +1（服务端、列表、Canvas 投放数）
const before = await countApi();
const spawnBefore = await ev("document.getElementById('player-mount')?._player?.danmaku?.spawnCount ?? -1");
await ev(`(() => {
  const i = document.querySelector('.dm-input');
  i.focus();
  i.value = '悬停测试弹幕';
  i.dispatchEvent(new Event('input', { bubbles: true }));
  i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return true;
})()`);
await sleep(1600);
const after = await countApi();
const uiRows = await ev("document.querySelectorAll('.dm-item').length");
const cleared = await ev("document.querySelector('.dm-input').value === ''");
const engineState = await ev(`(() => {
  const e = document.getElementById('player-mount')?._player?.danmaku;
  if (!e) return null;
  return {
    spawnDelta: e.spawnCount - ${spawnBefore},
    activeSame: e.active.filter((i) => i.text === '悬停测试弹幕').length,
    itemsSame: e.items.filter((i) => i.text === '悬停测试弹幕').length,
  };
})()`);
const dmCountText = await ev("document.getElementById('d-count')?.textContent || ''");

// 3) 快速来回切页 3 次后再发一条，验证监听器没有堆积
for (let i = 0; i < 3; i += 1) {
  await ev("document.querySelector('.brand').click()");
  await sleep(400);
  await send('Page.navigate', { url: `${base}/watch/${videoId}` }, sessionId);
  await sleep(900);
}
const before2 = await countApi();
await ev(`(() => {
  const i = document.querySelector('.dm-input');
  i.focus();
  i.value = '切换后只应发一条';
  i.dispatchEvent(new Event('input', { bubbles: true }));
  i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return true;
})()`);
await sleep(1800);
const after2 = await countApi();

const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
if (shot.result?.data) writeFileSync(out, Buffer.from(shot.result.data, 'base64'));

console.log(JSON.stringify({
  login,
  hover: hoverState,
  firstSend: { before, after, delta: after - before, uiRows, cleared, dmCountText, engineState },
  afterRapidNav: { before: before2, after: after2, delta: after2 - before2 },
  logs: logs.slice(0, 8),
}, null, 2));

await send('Target.closeTarget', { targetId });
ws.close();

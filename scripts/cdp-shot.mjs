// 极简 CDP 截图器：控制真实播放后截图，并收集控制台错误
// 用法: node scripts/cdp-shot.mjs <debugPort> <url> <out.png> [waitVideoSeconds]
import { writeFileSync } from 'node:fs';

const [, , portArg, url, out, waitArg] = process.argv;
const port = Number(portArg || 9222);
const waitVideo = Number(waitArg || 2.2);
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
  if (msg.method === 'Runtime.consoleAPICalled' && ['error', 'warning'].includes(msg.params.type)) {
    logs.push(`[console.${msg.params.type}] ${msg.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
  }
  if (msg.method === 'Runtime.exceptionThrown') {
    logs.push(`[exception] ${msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text}`);
  }
  if (msg.method === 'Log.entryAdded' && msg.params.entry.level === 'error') {
    logs.push(`[log] ${msg.params.entry.text} ${msg.params.entry.url || ''}`);
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

const created = await send('Target.createTarget', { url: 'about:blank' });
const targetId = created.result.targetId;
const attached = await send('Target.attachToTarget', { targetId, flatten: true });
const sessionId = attached.result.sessionId;

await send('Page.enable', {}, sessionId);
await send('Runtime.enable', {}, sessionId);
await send('Log.enable', {}, sessionId);
await send('Emulation.setDeviceMetricsOverride', {
  width: Number(process.env.SHOT_W || 1440),
  height: Number(process.env.SHOT_H || 1000),
  deviceScaleFactor: 1,
  mobile: false,
}, sessionId);

await send('Page.navigate', { url }, sessionId);
await sleep(1200);

async function evaluate(expression) {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId);
  return r.result?.result?.value;
}

// 触发播放并等待真实时间推进
await evaluate("(()=>{const v=document.querySelector('video');if(v){v.muted=false;v.play().catch(()=>{});}})()");
let state = null;
for (let i = 0; i < 50; i += 1) {
  state = await evaluate("(()=>{const v=document.querySelector('video');if(!v)return null;return {t:+v.currentTime.toFixed(2),paused:v.paused,ready:v.readyState,dur:+(v.duration||0).toFixed(2)};})()");
  if (state && state.t >= waitVideo) break;
  await sleep(120);
}

const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
if (!shot.result?.data) {
  console.error(JSON.stringify({ ok: false, error: shot.error, state, logs }));
  process.exit(1);
}
writeFileSync(out, Buffer.from(shot.result.data, 'base64'));

console.log(JSON.stringify({ ok: true, state, logs: logs.slice(0, 12) }, null, 2));

await send('Target.closeTarget', { targetId });
ws.close();

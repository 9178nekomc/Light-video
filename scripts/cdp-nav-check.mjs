// 路由切换体检：滚动复位、布局位移（CLS）、进度条、返回恢复
// 用法: node scripts/cdp-nav-check.mjs <debugPort> <baseUrl> <out.png>
import { writeFileSync } from 'node:fs';

const [, , portArg, base, out] = process.argv;
const port = Number(portArg || 9222);
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
await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false }, sessionId);

const ev = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId);
  return r.result?.result?.value;
};

await send('Page.navigate', { url: `${base}/` }, sessionId);
await sleep(1800);

// 记录布局位移
await ev(`(() => {
  window.__cls = 0;
  window.__shifts = [];
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        if (e.hadRecentInput) continue;
        window.__cls += e.value;
        if (e.value > 0.001) window.__shifts.push({ v: +e.value.toFixed(4), t: Math.round(e.startTime) });
      }
    }).observe({ type: 'layout-shift', buffered: true });
  } catch {}
  return true;
})()`);

const cards = await ev("document.querySelectorAll('.vcard').length");
await ev("window.scrollTo(0, 700)");
await sleep(250);
const beforeY = await ev('window.scrollY');

// 点击第 3 张卡片
await ev("document.querySelectorAll('.vcard')[2].click()");
await sleep(60);
const y60 = await ev('window.scrollY');
const bar60 = await ev("(() => { const b = document.getElementById('route-progress'); return b ? b.className : 'missing'; })()");
await sleep(300);
const y360 = await ev('window.scrollY');
await sleep(1200);
const y1560 = await ev('window.scrollY');
const route = await ev('location.pathname');
const cls = await ev('({ cls: +window.__cls.toFixed(4), shifts: window.__shifts.slice(0, 6) })');

// 返回上一页，检查滚动是否恢复
await ev('history.back()');
await sleep(1400);
const backY = await ev('window.scrollY');
const backPath = await ev('location.pathname');

const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
if (shot.result?.data) writeFileSync(out, Buffer.from(shot.result.data, 'base64'));

console.log(JSON.stringify({
  cards, beforeY, afterClick: { y60, y360, y1560, route }, progressBarAt60ms: bar60, cls,
  back: { path: backPath, scrollY: backY }, logs: logs.slice(0, 6),
}, null, 2));

await send('Target.closeTarget', { targetId });
ws.close();

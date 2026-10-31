// 极简 History 路由：滚动复位、转场动画、顶部进度条、请求代次保护
const routes = [];
let notFound = null;
let current = { path: '', cleanup: null };
let renderSeq = 0;
const scrollMap = new Map();

// 交给路由自己管理滚动，避免浏览器恢复位置与新页面高度打架
if ('scrollRestoration' in history) history.scrollRestoration = 'manual';

const keyOf = () => location.pathname + location.search;

function progressEl() {
  return document.getElementById('route-progress');
}

function progressStart() {
  const el = progressEl();
  if (!el) return;
  el.classList.remove('done');
  // 强制重排，让 scaleX 从 0 重新开始
  el.classList.remove('on');
  void el.offsetWidth;
  el.classList.add('on');
}

function progressDone() {
  const el = progressEl();
  if (!el) return;
  el.classList.remove('on');
  el.classList.add('done');
  setTimeout(() => el.classList.remove('done'), 320);
}

export function route(pattern, handler) {
  const keys = [];
  const regex = new RegExp(`^${pattern.replace(/:[^/]+/g, (m) => { keys.push(m.slice(1)); return '([^/]+)'; })}$`);
  routes.push({ regex, keys, handler, pattern });
}

export function setNotFound(handler) { notFound = handler; }

export function navigate(path, { replace = false } = {}) {
  if (path === keyOf()) return;
  scrollMap.set(keyOf(), window.scrollY);

  if (replace) history.replaceState({}, '', path);
  else history.pushState({}, '', path);

  // 新页面从顶部开始：不做这一步，就会看到「停在旧位置 → 内容加载后被顶回去」
  window.scrollTo({ top: 0, left: 0, behavior: 'instant' });
  render();
}

export async function render() {
  const my = ++renderSeq;
  const path = location.pathname;
  const query = Object.fromEntries(new URLSearchParams(location.search));
  const view = document.getElementById('view');
  if (!view) return;

  progressStart();
  const started = performance.now();

  if (typeof current.cleanup === 'function') {
    try { current.cleanup(); } catch (e) { console.error(e); }
  }
  current = { path, cleanup: null };

  let handled = false;
  for (const r of routes) {
    const m = path.match(r.regex);
    if (!m) continue;
    const params = {};
    r.keys.forEach((k, i) => { params[k] = decodeURIComponent(m[i + 1]); });
    const cleanup = await r.handler({ params, query, view });
    if (my !== renderSeq) {
      // 期间又发生了导航：本次结果作废，但监听器/定时器必须回收，
      // 否则每切一次页面就多一份 WS 订阅，弹幕会成倍重复
      if (typeof cleanup === 'function') {
        try { cleanup(); } catch (e) { console.error(e); }
      }
      return;
    }
    current.cleanup = typeof cleanup === 'function' ? cleanup : null;
    handled = true;
    break;
  }
  if (!handled && notFound) {
    await notFound({ view, path });
    if (my !== renderSeq) return;
  }

  // 入场动画：只重放一次，避免长列表逐项抖动
  view.classList.remove('route-enter');
  void view.offsetWidth;
  view.classList.add('route-enter');

  // 进度条至少露个脸，避免瞬间闪烁
  const elapsed = performance.now() - started;
  setTimeout(progressDone, Math.max(0, 140 - elapsed));
}

export function start() {
  document.addEventListener('click', (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const a = e.target.closest('a');
    if (!a) return;
    const href = a.getAttribute('href');
    if (!href || a.target === '_blank' || a.hasAttribute('download') || href.startsWith('http') || href.startsWith('#')) return;
    if (!href.startsWith('/') || href.startsWith('//')) return;
    if (a.dataset.noSpa !== undefined) return;
    e.preventDefault();
    navigate(href);
  });

  window.addEventListener('popstate', async () => {
    await render();
    const y = scrollMap.get(keyOf()) ?? 0;
    requestAnimationFrame(() => window.scrollTo(0, y));
  });

  render();
}

export function refresh() { render(); }

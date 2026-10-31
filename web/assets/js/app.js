import { api, onAuthLost } from './api.js';
import { ws } from './ws.js';
import { store, toggleTheme } from './store.js';
import { icon, esc, avatar, toast, fmtCount, debounce } from './ui.js';
import { route, start, navigate, setNotFound } from './router.js';
import { renderHome } from './pages/home.js';
import { renderWatch } from './pages/watch.js';
import { renderUpload } from './pages/upload.js';
import { renderProfile } from './pages/profile.js';
import { renderFriends } from './pages/friends.js';
import { renderChat } from './pages/chat.js';
import { renderNotifications } from './pages/notifications.js';
import { renderAuth } from './pages/auth.js';

// ==================== 顶栏 ====================

function topActions() {
  const el = document.getElementById('top-actions');
  const u = store.user;
  if (!u) {
    el.innerHTML = `
      <a class="btn btn-ghost" href="/login">登录</a>
      <a class="btn btn-primary" href="/register">注册</a>`;
    return;
  }
  const { unread_messages = 0, unread_notifications = 0 } = store.stats;
  el.innerHTML = `
    <a class="btn btn-primary" href="/upload" title="投稿">
      ${icon('upload')}<span class="btn-upload-label">投稿</span>
    </a>
    <a class="icon-btn" href="/chat" title="私信" aria-label="私信">
      ${icon('chat')}<span class="badge" data-badge="msg" ${unread_messages ? '' : 'hidden'}>${unread_messages > 99 ? '99+' : (unread_messages || '')}</span>
    </a>
    <a class="icon-btn" href="/notifications" title="通知" aria-label="通知">
      ${icon('bell')}<span class="badge" data-badge="notif" ${unread_notifications ? '' : 'hidden'}>${unread_notifications > 99 ? '99+' : (unread_notifications || '')}</span>
    </a>
    <div class="user-wrap">
      <button class="avatar-btn" id="avatar-btn" aria-haspopup="menu" aria-expanded="false">
        ${avatar(u)}
        <span class="hide-sm" style="font-size:13.5px;font-weight:600;max-width:88px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(u.display_name || u.username)}</span>
        ${icon('chevronDown', 'chev')}
      </button>
      <div class="menu" id="user-menu" hidden>
        <div class="menu-head">
          ${avatar(u)}
          <div>
            <div class="mh-name">${esc(u.display_name || u.username)}</div>
            <div class="mh-sub">@${esc(u.username)}</div>
          </div>
        </div>
        <div class="menu-sep"></div>
        <a href="/user/${u.id}">${icon('user')}个人主页</a>
        <a href="/user/${u.id}?tab=favorites">${icon('star')}我的收藏</a>
        <a href="/friends">${icon('friends')}好友</a>
        <a href="/upload">${icon('upload')}投稿</a>
        <div class="menu-sep"></div>
        <button data-theme-toggle>${icon(store.theme === 'dark' ? 'sun' : 'moon')}${store.theme === 'dark' ? '浅色模式' : '深色模式'}</button>
        <button data-logout>${icon('logout')}退出登录</button>
      </div>
    </div>`;

  const btn = el.querySelector('#avatar-btn');
  const menu = el.querySelector('#user-menu');
  btn.addEventListener('click', (e) => {
    e.stopPropagation();
    menu.hidden = !menu.hidden;
    btn.setAttribute('aria-expanded', String(!menu.hidden));
  });
  document.addEventListener('click', () => { menu.hidden = true; });
  menu.addEventListener('click', (e) => e.stopPropagation());
  menu.querySelector('[data-theme-toggle]').addEventListener('click', () => {
    const t = toggleTheme();
    menu.querySelector('[data-theme-toggle]').innerHTML = `${icon(t === 'dark' ? 'sun' : 'moon')}${t === 'dark' ? '浅色模式' : '深色模式'}`;
  });
  menu.querySelector('[data-logout]').addEventListener('click', async () => {
    try {
      await api('/api/auth/logout', { method: 'POST' });
    } catch { /* 忽略 */ }
    store.setUser(null, { unread_messages: 0, unread_notifications: 0 });
    toast('已退出登录', 'ok');
    navigate('/');
  });
}

function paintBadge(kind, count) {
  const el = document.querySelector(`[data-badge="${kind}"]`);
  if (!el) return;
  el.hidden = !count;
  el.textContent = count > 99 ? '99+' : String(count);
}

store.on('user', topActions);
store.on('stats', (stats) => {
  paintBadge('msg', stats.unread_messages || 0);
  paintBadge('notif', stats.unread_notifications || 0);
});

// ==================== 搜索 ====================

function setupSearch() {
  const form = document.getElementById('search-form');
  const input = document.getElementById('search-input');
  const panel = document.getElementById('search-panel');

  const run = debounce(async () => {
    const q = input.value.trim();
    if (q.length < 1) { panel.hidden = true; return; }
    try {
      const [videos, users] = await Promise.all([
        api(`/api/videos?q=${encodeURIComponent(q)}&limit=6`),
        api(`/api/users/search?q=${encodeURIComponent(q)}`),
      ]);
      const vItems = (videos?.items || []).map((v) => `
        <a href="/watch/${v.id}">
          <span class="avatar-fallback" style="width:54px;height:32px;border-radius:6px;font-size:10px">${icon('play')}</span>
          <span style="min-width:0"><span style="display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(v.title)}</span>
          <span style="font-size:11px;color:var(--text-mute)">${fmtCount(v.views)} 播放</span></span>
        </a>`).join('');
      const uItems = (users || []).slice(0, 4).map((u) => `
        <a href="/user/${u.id}">
          ${avatar(u, 'avatar-sm')}
          <span>${esc(u.display_name || u.username)}<span style="color:var(--text-mute);font-size:11.5px"> @${esc(u.username)}</span></span>
        </a>`).join('');
      panel.innerHTML = `
        ${vItems ? `<div class="sp-title">视频</div>${vItems}` : ''}
        ${uItems ? `<div class="sp-title">用户</div>${uItems}` : ''}
        ${!vItems && !uItems ? '<div class="sp-empty">没有找到相关内容</div>' : ''}`;
      panel.hidden = false;
    } catch {
      panel.hidden = true;
    }
  }, 260);

  input.addEventListener('input', run);
  input.addEventListener('focus', () => { if (input.value.trim()) run(); });
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const q = input.value.trim();
    if (!q) return;
    panel.hidden = true;
    navigate(`/?q=${encodeURIComponent(q)}`);
  });
  document.addEventListener('click', (e) => {
    if (!e.target.closest('.search')) panel.hidden = true;
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === '/' && !/INPUT|TEXTAREA/.test(document.activeElement?.tagName || '')) {
      e.preventDefault();
      input.focus();
    }
    if (e.key === 'Escape') panel.hidden = true;
  });
}

// ==================== 启动 ====================

async function boot() {
  await store.load();
  topActions();
  setupSearch();

  // 顶栏滚动态
  const topbar = document.getElementById('topbar');
  const onScroll = () => topbar.classList.toggle('scrolled', window.scrollY > 6);
  window.addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  // 页脚统计
  api('/api/stats', { silent: true }).then((s) => {
    if (!s) return;
    document.getElementById('footer-stats').textContent =
      `${fmtCount(s.videos)} 个视频 · ${fmtCount(s.users)} 位创作者 · ${fmtCount(s.danmaku)} 条弹幕`;
  }).catch(() => {});

  // 会话失效
  onAuthLost(async () => {
    store.setUser(null, { unread_messages: 0, unread_notifications: 0 });
    toast('登录已过期，请重新登录', 'err');
  });

  // WS 实时角标
  ws.on('notification', (msg) => {
    store.patchStats({ unread_notifications: (store.stats.unread_notifications || 0) + 1 });
    const text = msg.data?.content || '你有一条新通知';
    toast(text, 'info');
  });
  ws.on('chat', () => {
    if (!location.pathname.startsWith('/chat')) {
      store.patchStats({ unread_messages: (store.stats.unread_messages || 0) + 1 });
    }
  });
  ws.onState((s) => {
    document.body.dataset.ws = s;
  });
  ws.connect();

  // 路由
  route('/', renderHome);
  route('/watch/:id', renderWatch);
  route('/upload', renderUpload);
  route('/user/:id', renderProfile);
  route('/friends', renderFriends);
  route('/chat', renderChat);
  route('/chat/:id', renderChat);
  route('/notifications', renderNotifications);
  route('/login', (ctx) => renderAuth({ ...ctx, mode: 'login' }));
  route('/register', (ctx) => renderAuth({ ...ctx, mode: 'register' }));
  setNotFound(({ view }) => {
    view.innerHTML = `<div class="empty" style="padding-top:120px">
      ${icon('alert')}<strong>页面不存在</strong>
      <a class="btn" href="/">回到首页</a></div>`;
  });
  start();
}

boot();

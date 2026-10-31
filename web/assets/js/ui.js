// 轻量 UI 工具：图标、toast、弹窗、格式化
const ICONS = {
  search: '<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',
  upload: '<path d="M12 16V4"/><path d="m7 9 5-5 5 5"/><path d="M4 16v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>',
  bell: '<path d="M18 9a6 6 0 1 0-12 0c0 5-2 6-2 6h16s-2-1-2-6"/><path d="M10.5 20a2 2 0 0 0 3 0"/>',
  comment: '<path d="M20 12a7.5 7.5 0 0 1-10.9 6.7L4 20l1.3-4.1A7.5 7.5 0 1 1 20 12z"/>',
  chat: '<path d="M4 6h16v10H9l-5 4V6z"/>',
  friends: '<circle cx="9" cy="8" r="3.2"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M16 5.2a3.2 3.2 0 0 1 0 5.6"/><path d="M17.5 14.2A6.5 6.5 0 0 1 21.5 20"/>',
  user: '<circle cx="12" cy="8" r="3.6"/><path d="M4.5 20.5a7.5 7.5 0 0 1 15 0"/>',
  logout: '<path d="M15 17l5-5-5-5"/><path d="M20 12H9"/><path d="M12 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h6"/>',
  play: '<path d="M8 5.5c0-.9 1-1.4 1.7-.9l9 6.5c.6.4.6 1.4 0 1.8l-9 6.5c-.7.5-1.7 0-1.7-.9V5.5z" fill="currentColor" stroke="none"/>',
  pause: '<rect x="6.5" y="5" width="4" height="14" rx="1.2" fill="currentColor" stroke="none"/><rect x="13.5" y="5" width="4" height="14" rx="1.2" fill="currentColor" stroke="none"/>',
  volume: '<path d="M11 5 6.5 9H3v6h3.5L11 19V5z"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M18.5 6a8.5 8.5 0 0 1 0 12"/>',
  mute: '<path d="M11 5 6.5 9H3v6h3.5L11 19V5z"/><path d="m16 9 5 6"/><path d="m21 9-5 6"/>',
  fullscreen: '<path d="M4 9V4h5"/><path d="M20 15v5h-5"/><path d="M20 9V4h-5"/><path d="M4 15v5h5"/>',
  exitFullscreen: '<path d="M9 4v5H4"/><path d="M15 20v-5h5"/><path d="M15 4v5h5"/><path d="M9 20v-5H4"/>',
  pip: '<rect x="3" y="5" width="18" height="13" rx="2"/><rect x="12" y="11" width="7" height="5" rx="1"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1A1.6 1.6 0 0 0 9 19.4a1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1A1.6 1.6 0 0 0 4.6 9a1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3H9a1.6 1.6 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8V9a1.6 1.6 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1z"/>',
  danmaku: '<path d="M3 6h18v10H9l-6 4V6z"/><path d="M7 10h7"/><path d="M7 13h4"/>',
  heart: '<path d="M12 20s-7-4.3-7-9a4 4 0 0 1 7-2.6A4 4 0 0 1 19 11c0 4.7-7 9-7 9z"/>',
  star: '<path d="m12 3.8 2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8L3.5 10l5.9-.9L12 3.8z"/>',
  share: '<circle cx="18" cy="5" r="2.6"/><circle cx="6" cy="12" r="2.6"/><circle cx="18" cy="19" r="2.6"/><path d="m8.3 10.8 7.4-4.3"/><path d="m8.3 13.2 7.4 4.3"/>',
  send: '<path d="M21 3 10.5 13.5"/><path d="M21 3 14.5 21l-4-7.5L3 9.5 21 3z"/>',
  trash: '<path d="M4 7h16"/><path d="M9 7V4.5h6V7"/><path d="M6.5 7 7.5 20h9L17.5 7"/><path d="M10.5 11v5M13.5 11v5"/>',
  close: '<path d="m6 6 12 12"/><path d="m18 6-12 12"/>',
  check: '<path d="m4.5 12.5 5 5 10-11"/>',
  clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7v5.2l3.2 2"/>',
  eye: '<path d="M2.5 12S6 5.8 12 5.8 21.5 12 21.5 12 18 18.2 12 18.2 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="2.8"/>',
  video: '<rect x="2.5" y="5.5" width="13" height="13" rx="2.5"/><path d="m15.5 10.5 6-3.2v9.4l-6-3.2z"/>',
  home: '<path d="m3.5 10.5 8.5-7 8.5 7"/><path d="M5.5 9.5V20h13V9.5"/>',
  plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
  image: '<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><circle cx="8.5" cy="10" r="1.8"/><path d="m4 17 4.5-4.5L13 17l3-3 4 4"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M19.1 4.9l-1.4 1.4M6.3 17.7l-1.4 1.4"/>',
  moon: '<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  chevronLeft: '<path d="m15 5-6 7 6 7"/>',
  more: '<circle cx="5.5" cy="12" r="1.4" fill="currentColor"/><circle cx="12" cy="12" r="1.4" fill="currentColor"/><circle cx="18.5" cy="12" r="1.4" fill="currentColor"/>',
  checkCircle: '<circle cx="12" cy="12" r="9"/><path d="m8 12.5 2.6 2.6L16 9.6"/>',
  alert: '<circle cx="12" cy="12" r="9"/><path d="M12 7.5v5.5"/><path d="M12 16.5h.01"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><path d="M12 7.8h.01"/>',
  wifi: '<path d="M4 9.5a12 12 0 0 1 16 0"/><path d="M7 13a8 8 0 0 1 10 0"/><path d="M10 16.3a4 4 0 0 1 4 0"/><circle cx="12" cy="19.5" r="1" fill="currentColor"/>',
  empty: '<rect x="3" y="5" width="18" height="14" rx="3"/><path d="M3 10h18"/><path d="M8 5v5"/>',
  sparkles: '<path d="m12 3 1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6L12 3z"/><path d="M18.5 15.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8.8-2.2z"/>',
  users: '<circle cx="9" cy="8" r="3.2"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M17 5.3a3.2 3.2 0 0 1 0 5.4"/>',
  flash: '<path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z"/>',
};

export function icon(name, cls = '') {
  const body = ICONS[name] || '';
  return `<svg viewBox="0 0 24 24" class="${cls}" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;
}

export function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

export function fmtCount(n) {
  n = Number(n) || 0;
  if (n < 1000) return String(n);
  if (n < 10000) return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k';
  if (n < 100000000) return (n / 10000).toFixed(1).replace(/\.0$/, '') + '万';
  return (n / 100000000).toFixed(1).replace(/\.0$/, '') + '亿';
}

export function fmtDuration(ms) {
  if (!ms || ms < 0) return '0:00';
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
    : `${m}:${String(s).padStart(2, '0')}`;
}

export function fmtBytes(b) {
  if (!b) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let v = b;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i += 1; }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function timeAgo(iso) {
  if (!iso) return '';
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return '';
  const diff = (Date.now() - t) / 1000;
  if (diff < 60) return '刚刚';
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`;
  if (diff < 86400 * 365) return `${Math.floor(diff / (86400 * 30))} 个月前`;
  return `${Math.floor(diff / (86400 * 365))} 年前`;
}

export function fmtDate(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

// 无头像时用单一中性色块，避免花哨渐变
const AVATAR_COLORS = ['#fb7299', '#7b8794', '#c9974d', '#5c9c86', '#a4738f'];

export function avatar(user, cls = 'avatar') {
  const name = user?.display_name || user?.username || '?';
  if (user?.avatar_url) {
    return `<img class="${cls}" src="${esc(user.avatar_url)}" alt="${esc(name)}" loading="lazy">`;
  }
  const idx = [...String(name)].reduce((a, c) => a + c.charCodeAt(0), 0) % AVATAR_COLORS.length;
  return `<span class="${cls} avatar-fallback" style="background:${AVATAR_COLORS[idx]}">${esc(String(name).slice(0, 1).toUpperCase())}</span>`;
}

export function toast(message, kind = 'info', ms = 2800) {
  const root = document.getElementById('toasts');
  if (!root) return;
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  const ico = kind === 'ok' ? 'checkCircle' : kind === 'err' ? 'alert' : 'info';
  el.innerHTML = `${icon(ico)}<span>${esc(message)}</span>`;
  root.appendChild(el);
  const close = () => {
    el.classList.add('out');
    setTimeout(() => el.remove(), 220);
  };
  setTimeout(close, ms);
  el.addEventListener('click', close);
}

export function confirmDialog({ title, message, confirmText = '确定', danger = false }) {
  return new Promise((resolve) => {
    const root = document.getElementById('modal-root');
    const mask = document.createElement('div');
    mask.className = 'modal-mask';
    mask.innerHTML = `
      <div class="modal" role="dialog" aria-modal="true">
        <h3>${esc(title)}</h3>
        <p>${esc(message)}</p>
        <div class="modal-actions">
          <button class="btn" data-no>取消</button>
          <button class="btn ${danger ? '' : 'btn-primary'}" data-yes style="${danger ? 'border-color:rgba(255,95,95,.5);color:#ff5f5f' : ''}">${esc(confirmText)}</button>
        </div>
      </div>`;
    root.appendChild(mask);
    const done = (v) => { mask.remove(); resolve(v); };
    mask.querySelector('[data-no]').onclick = () => done(false);
    mask.querySelector('[data-yes]').onclick = () => done(true);
    mask.addEventListener('click', (e) => { if (e.target === mask) done(false); });
    document.addEventListener('keydown', function onKey(e) {
      if (e.key === 'Escape') { document.removeEventListener('keydown', onKey); done(false); }
    });
  });
}

export function skeletonGrid(n = 10) {
  return `<div class="grid">${Array.from({ length: n }).map(() => `
    <div class="vcard" aria-hidden="true">
      <div class="vcard-media skeleton"></div>
      <div class="skeleton skeleton-line"></div>
      <div class="skeleton skeleton-line short"></div>
      <div class="skeleton skeleton-meta"></div>
    </div>`).join('')}</div>`;
}

/** 列表骨架：与 .list-row 高度一致，加载完成后不跳动 */
export function skeletonRows(n = 3, height = 64) {
  return Array.from({ length: n }).map(() => `
    <div class="list-row" style="height:${height}px" aria-hidden="true">
      <div class="skeleton" style="width:38px;height:38px;border-radius:50%"></div>
      <div class="grow">
        <div class="skeleton skeleton-line" style="width:36%"></div>
        <div class="skeleton skeleton-line short" style="margin-top:7px"></div>
      </div>
    </div>`).join('');
}

export function emptyState(text, sub = '', ico = 'empty') {
  return `<div class="empty">${icon(ico)}<strong>${esc(text)}</strong>${sub ? `<span>${esc(sub)}</span>` : ''}</div>`;
}

/** 生成一个比 div 更安全的模板：只做字符串拼接，避免 XSS 时统一走 esc() */
export function html(strings, ...vals) {
  return strings.reduce((out, s, i) => out + s + (i < vals.length ? String(vals[i] ?? '') : ''), '');
}

export function debounce(fn, ms = 250) {
  let t;
  return (...args) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...args), ms);
  };
}

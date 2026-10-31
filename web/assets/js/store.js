// 全局状态：登录用户、统计数据、主题
const bus = new EventTarget();

export const store = {
  user: null,
  stats: { unread_messages: 0, unread_notifications: 0, friends: 0, videos: 0 },
  ready: false,
  theme: localStorage.getItem('lv:theme') || 'light',

  on(evt, fn) {
    const h = (e) => fn(e.detail);
    bus.addEventListener(evt, h);
    return () => bus.removeEventListener(evt, h);
  },
  emit(evt, detail) {
    bus.dispatchEvent(new CustomEvent(evt, { detail }));
  },

  setUser(user, stats) {
    this.user = user || null;
    if (stats) this.stats = { ...this.stats, ...stats };
    this.emit('user', this.user);
  },

  patchStats(patch) {
    this.stats = { ...this.stats, ...patch };
    this.emit('stats', this.stats);
  },

  async load() {
    const { api } = await import('./api.js');
    try {
      const data = await api('/api/auth/me', { silent: true });
      if (data?.user) {
        this.user = data.user;
        this.stats = { ...this.stats, ...(data.stats || {}) };
      } else {
        this.user = null;
      }
    } catch {
      this.user = null;
    }
    this.ready = true;
    this.emit('user', this.user);
    this.emit('stats', this.stats);
    return this.user;
  },

  isLoggedIn() {
    return !!this.user;
  },
};

export function applyTheme(theme) {
  store.theme = theme;
  document.documentElement.setAttribute('data-theme', theme);
  localStorage.setItem('lv:theme', theme);
  const meta = document.querySelector('meta[name="color-scheme"]');
  if (meta) meta.setAttribute('content', theme === 'dark' ? 'dark light' : 'light dark');
}

export function toggleTheme() {
  applyTheme(store.theme === 'dark' ? 'light' : 'dark');
  return store.theme;
}

applyTheme(store.theme);

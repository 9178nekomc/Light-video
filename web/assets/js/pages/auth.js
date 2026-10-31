import { api } from '../api.js';
import { store } from '../store.js';
import { icon, toast } from '../ui.js';
import { navigate } from '../router.js';

export async function renderAuth({ view, mode, query }) {
  const isLogin = mode === 'login';
  const next = query.next || '/';

  if (store.user) {
    view.innerHTML = `<div class="auth-wrap"><div class="auth-card" style="text-align:center">
      <h1>已登录</h1><p class="sub">当前账号：${store.user.username}</p>
      <a class="btn btn-primary btn-block" href="/">回到首页</a>
    </div></div>`;
    return;
  }

  view.innerHTML = `
    <div class="auth-wrap">
      <form class="auth-card" id="auth-form" novalidate>
        <h1>${isLogin ? '欢迎回来' : '创建账号'}</h1>
        <p class="sub">${isLogin ? '继续你的视频之旅' : '加入轻视频，开始分享与交流'}</p>

        ${isLogin ? '' : `
        <div class="field">
          <label for="a-email">邮箱</label>
          <input class="input" id="a-email" type="email" autocomplete="email" placeholder="you@example.com" required>
        </div>`}

        <div class="field">
          <label for="a-user">${isLogin ? '用户名或邮箱' : '用户名'}</label>
          <input class="input" id="a-user" autocomplete="username" placeholder="${isLogin ? '用户名 / 邮箱' : '3-20 位，支持中英文'}" required>
        </div>

        <div class="field">
          <label for="a-pass">密码</label>
          <input class="input" id="a-pass" type="password" autocomplete="${isLogin ? 'current-password' : 'new-password'}" placeholder="至少 8 位，含字母和数字" required>
          ${isLogin ? '' : '<div class="pw-meter"><i></i><i></i><i></i><i></i></div>'}
        </div>

        ${isLogin ? `
        <label style="display:flex;align-items:center;gap:8px;font-size:13px;color:var(--text-dim);margin-bottom:6px">
          <input type="checkbox" id="a-remember" checked> 保持登录
        </label>` : ''}

        <div class="error-text" id="a-error"></div>
        <button class="btn btn-primary btn-block btn-lg" type="submit" id="a-submit">${isLogin ? '登录' : '注册并登录'}</button>

        <div class="foot">
          ${isLogin
            ? `还没有账号？<a href="/register${next !== '/' ? `?next=${encodeURIComponent(next)}` : ''}">立即注册</a>`
            : `已有账号？<a href="/login${next !== '/' ? `?next=${encodeURIComponent(next)}` : ''}">去登录</a>`}
        </div>
      </form>
    </div>`;

  const form = document.getElementById('auth-form');
  const err = document.getElementById('a-error');
  const pass = document.getElementById('a-pass');
  const meter = document.querySelector('.pw-meter');

  pass.addEventListener('input', () => {
    if (!meter) return;
    const v = pass.value;
    let score = 0;
    if (v.length >= 8) score += 1;
    if (/[a-z]/i.test(v) && /\d/.test(v)) score += 1;
    if (v.length >= 12) score += 1;
    if (/[^a-zA-Z0-9]/.test(v)) score += 1;
    [...meter.children].forEach((el, i) => {
      el.style.background = i < score
        ? ['#ff5f5f', '#f59e0b', '#34d399', '#34e2c4'][Math.max(0, score - 1)]
        : 'var(--surface-3)';
    });
  });

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    err.textContent = '';
    const submit = document.getElementById('a-submit');
    const username = document.getElementById('a-user').value.trim();
    const password = pass.value;
    if (!username || !password) { err.textContent = '请填写完整信息'; return; }
    if (!isLogin) {
      const email = document.getElementById('a-email').value.trim();
      if (!/^[^@\s]+@[^@\s.]+\.[^@\s]+$/.test(email)) { err.textContent = '邮箱格式不正确'; return; }
      if (password.length < 8) { err.textContent = '密码至少 8 位'; return; }
    }

    submit.disabled = true;
    submit.innerHTML = '<span class="spinner"></span>';
    try {
      if (isLogin) {
        await api('/api/auth/login', { method: 'POST', body: { username, password, remember: document.getElementById('a-remember').checked } });
      } else {
        await api('/api/auth/register', {
          method: 'POST',
          body: { username, email: document.getElementById('a-email').value.trim(), password },
        });
      }
      await store.load();
      toast(isLogin ? '登录成功' : '注册成功，欢迎加入', 'ok');
      navigate(next.startsWith('/') ? next : '/');
    } catch (e2) {
      err.textContent = e2.message || '操作失败';
      submit.disabled = false;
      submit.innerHTML = isLogin ? '登录' : '注册并登录';
    }
  });

  return () => { document.title = '轻视频 · Light Video'; };
}

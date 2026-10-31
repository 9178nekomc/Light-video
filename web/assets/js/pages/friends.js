import { api } from '../api.js';
import { store } from '../store.js';
import { icon, esc, avatar, toast, emptyState, debounce, skeletonRows } from '../ui.js';
import { userRow } from '../cards.js';
import { navigate } from '../router.js';

export async function renderFriends({ view }) {
  if (!store.user) { navigate('/login?next=/friends'); return; }

  view.innerHTML = `
    <div class="page-wide">
    <div class="section-head" style="margin-top:0">
      <h2>好友</h2>
      <span class="hint">互相关注后即可私信</span>
    </div>
    <div class="two-col">
      <div>
        <div class="field" style="margin-bottom:14px">
          <input class="input" id="u-search" placeholder="搜索用户名，添加新好友…" autocomplete="off">
        </div>
        <div id="search-result"></div>
        <div class="list-card" id="friend-list">${skeletonRows(3)}</div>
      </div>
      <aside>
        <div class="side-title"><span>好友请求</span><span id="req-count"></span></div>
        <div class="list-card" id="request-list">${skeletonRows(2)}</div>
      </aside>
    </div>
    </div>`;

  const list = document.getElementById('friend-list');
  const requests = document.getElementById('request-list');
  const result = document.getElementById('search-result');

  async function loadFriends() {
    try {
      const rows = await api('/api/friends');
      if (!rows.length) {
        list.innerHTML = emptyState('还没有好友', '搜索用户名，发送第一条好友请求', 'users');
        return;
      }
      list.innerHTML = rows.map((u) => userRow(u, `
        <a class="btn btn-sm" href="/chat/${u.id}">${icon('chat')} 私信</a>
        <button class="icon-btn" data-remove="${u.id}" title="删除好友" style="width:32px;height:32px">${icon('trash')}</button>
      `)).join('');
    } catch (e) {
      list.innerHTML = emptyState('加载失败', e.message, 'alert');
    }
  }

  async function loadRequests() {
    try {
      const rows = await api('/api/friends/requests');
      document.getElementById('req-count').textContent = rows.length ? `${rows.length} 条` : '';
      requests.innerHTML = rows.length
        ? rows.map((u) => `
          <div class="list-row">
            <a href="/user/${u.id}">${avatar(u)}</a>
            <div class="grow">
              <a class="nm" href="/user/${u.id}">${esc(u.display_name || u.username)}</a>
              <div class="sb">@${esc(u.username)}</div>
            </div>
            <button class="btn btn-sm btn-primary" data-accept="${u.id}">接受</button>
            <button class="btn btn-sm" data-reject="${u.id}">拒绝</button>
          </div>`).join('')
        : '<div class="empty" style="padding:26px">暂无待处理请求</div>';
    } catch {
      requests.innerHTML = '<div class="empty" style="padding:26px">加载失败</div>';
    }
  }

  list.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-remove]');
    if (!btn) return;
    try {
      await api(`/api/users/${btn.dataset.remove}/friend`, { method: 'DELETE' });
      toast('已删除好友', 'ok');
      loadFriends();
    } catch (err) { toast(err.message, 'err'); }
  });

  requests.addEventListener('click', async (e) => {
    const acc = e.target.closest('[data-accept]');
    const rej = e.target.closest('[data-reject]');
    try {
      if (acc) {
        await api(`/api/users/${acc.dataset.accept}/friend-accept`, { method: 'POST' });
        toast('已接受好友请求', 'ok');
      } else if (rej) {
        await api(`/api/users/${rej.dataset.reject}/friend-reject`, { method: 'POST' });
        toast('已拒绝', 'info');
      } else return;
      loadRequests();
      loadFriends();
    } catch (err) { toast(err.message, 'err'); }
  });

  const search = debounce(async (q) => {
    if (!q) { result.innerHTML = ''; return; }
    try {
      const rows = await api(`/api/users/search?q=${encodeURIComponent(q)}`);
      result.innerHTML = rows.length
        ? `<div class="list-card" style="margin-bottom:14px">${rows.map((u) => userRow(u, `
            <a class="btn btn-sm" href="/user/${u.id}">主页</a>
            <button class="btn btn-sm btn-primary" data-add="${u.id}">加好友</button>
          `)).join('')}</div>`
        : '<div class="hint" style="margin-bottom:14px">没有找到用户</div>';
    } catch (e) {
      result.innerHTML = `<div class="hint" style="margin-bottom:14px">${esc(e.message)}</div>`;
    }
  }, 280);

  document.getElementById('u-search').addEventListener('input', (e) => search(e.target.value.trim()));
  result.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-add]');
    if (!btn) return;
    try {
      const r = await api(`/api/users/${btn.dataset.add}/friend-request`, { method: 'POST' });
      btn.textContent = r.status === 'accepted' ? '已添加' : '已发送';
      btn.disabled = true;
      toast(r.status === 'accepted' ? '已成为好友' : '请求已发送', 'ok');
    } catch (err) { toast(err.message, 'err'); }
  });

  await Promise.all([loadFriends(), loadRequests()]);

  // 实时刷新好友请求
  const off = import('../ws.js').then(({ ws }) => ws.on('notification', (msg) => {
    if (msg.data?.kind?.startsWith('friend')) { loadRequests(); }
  }));
  return () => { off.then((fn) => fn?.()); };
}

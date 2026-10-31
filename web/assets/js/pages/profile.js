import { api } from '../api.js';
import { store } from '../store.js';
import { icon, esc, fmtCount, fmtDate, avatar, toast, confirmDialog, emptyState, skeletonGrid } from '../ui.js';
import { videoCard } from '../cards.js';
import { navigate } from '../router.js';

export async function renderProfile({ view, params, query }) {
  const id = Number(params.id);
  view.innerHTML = `
    <section class="profile-hero">
      <div class="profile-main">
        <div class="avatar-lg skeleton"></div>
        <div class="info">
          <div class="skeleton" style="height:26px;width:180px"></div>
          <div class="skeleton" style="height:14px;width:260px;margin-top:10px"></div>
          <div class="skeleton" style="height:14px;width:min(60%,420px);margin-top:12px"></div>
          <div class="skeleton" style="height:36px;width:320px;margin-top:16px"></div>
        </div>
      </div>
    </section>
    ${skeletonGrid(8)}`;

  let data;
  try {
    data = await api(`/api/users/${id}`);
  } catch (e) {
    view.innerHTML = emptyState('用户不存在', e.message, 'alert');
    return;
  }

  const { user, stats, relation, is_self: isSelf, joined_at: joinedAt, online } = data;
  const tab = query.tab || 'videos';
  document.title = `${user.display_name || user.username} · 轻视频`;

  view.innerHTML = `
    <section class="profile-hero">
      <div class="profile-main">
        ${avatar(user, 'avatar-lg')}
        <div class="info">
          <h1>${esc(user.display_name || user.username)}
            ${online ? '<span class="online-dot" title="在线"></span>' : ''}
          </h1>
          <div class="handle">@${esc(user.username)} · 加入于 ${fmtDate(joinedAt)}</div>
          <div class="bio">${esc(user.bio) || '<span style="color:var(--text-mute)">这个人很懒，还没有填写简介</span>'}</div>
          <div class="profile-stats">
            <div><b>${fmtCount(stats.videos)}</b>作品</div>
            <div><b>${fmtCount(stats.views)}</b>播放</div>
            <div><b>${fmtCount(stats.likes)}</b>获赞</div>
            <div><b>${fmtCount(stats.friends)}</b>好友</div>
          </div>
        </div>
        <div style="display:flex;gap:8px;flex-wrap:wrap;align-items:center">
          ${isSelf
            ? `<button class="btn" id="edit-btn">${icon('settings')} 编辑资料</button>
               <a class="btn btn-primary" href="/upload">${icon('upload')} 投稿</a>`
            : `${relation.is_friend
                ? `<a class="btn btn-primary" href="/chat/${user.id}">${icon('chat')} 发私信</a>`
                : relation.has_pending
                  ? '<button class="btn" disabled>请求已发送</button>'
                  : `<button class="btn btn-primary" id="add-friend">${icon('friends')} 加好友</button>`}
               <button class="btn" id="more-btn">${icon('more')}</button>`}
        </div>
      </div>
    </section>

    <div class="tabbar" id="tabs">
      <button class="${tab === 'videos' ? 'active' : ''}" data-tab="videos">作品</button>
      ${isSelf ? `<button class="${tab === 'favorites' ? 'active' : ''}" data-tab="favorites">收藏</button>
      <button class="${tab === 'history' ? 'active' : ''}" data-tab="history">历史</button>` : ''}
    </div>
    <div id="tab-content"></div>`;

  const content = document.getElementById('tab-content');
  const loadTab = async (name) => {
    content.innerHTML = skeletonGrid(8);
    try {
      let res;
      if (name === 'videos') {
        res = { items: data.videos || [] };
      } else if (name === 'favorites') {
        res = await api('/api/me/favorites?limit=40');
      } else {
        res = await api('/api/me/history?limit=40');
      }
      const items = res.items || [];
      if (!items.length) {
        content.innerHTML = name === 'videos'
          ? emptyState('还没有作品', isSelf ? '上传第一个视频吧' : '', 'video')
          : name === 'favorites' ? emptyState('还没有收藏', '看到喜欢的视频点个收藏', 'star')
          : emptyState('还没有观看记录', '', 'clock');
        return;
      }
      content.innerHTML = `<div class="grid">${items.map(videoCard).join('')}</div>`;
    } catch (e) {
      content.innerHTML = emptyState('加载失败', e.message, 'alert');
    }
  };

  document.getElementById('tabs').addEventListener('click', (e) => {
    const b = e.target.closest('[data-tab]');
    if (!b) return;
    document.querySelectorAll('#tabs button').forEach((x) => x.classList.toggle('active', x === b));
    history.replaceState({}, '', `/user/${id}?tab=${b.dataset.tab}`);
    loadTab(b.dataset.tab);
  });

  document.getElementById('add-friend')?.addEventListener('click', async (e) => {
    if (!store.user) { navigate('/login'); return; }
    const btn = e.currentTarget;
    try {
      const r = await api(`/api/users/${id}/friend-request`, { method: 'POST' });
      btn.textContent = r.status === 'accepted' ? '已是好友' : '请求已发送';
      btn.disabled = true;
      toast(r.status === 'accepted' ? '你们已成为好友' : '好友请求已发送', 'ok');
    } catch (err) { toast(err.message, 'err'); }
  });

  document.getElementById('more-btn')?.addEventListener('click', async () => {
    const ok = await confirmDialog({ title: '屏蔽该用户？', message: '当前版本会将其从推荐中隐藏（演示）。', danger: true, confirmText: '确定' });
    if (ok) toast('已记录', 'ok');
  });

  document.getElementById('edit-btn')?.addEventListener('click', () => openEdit(user, view, () => navigate(`/user/${id}`)));

  await loadTab(tab);
  return () => { document.title = '轻视频 · Light Video'; };
}

function openEdit(user, view, done) {
  const mask = document.createElement('div');
  mask.className = 'modal-mask';
  mask.innerHTML = `
    <div class="modal" style="width:min(520px,100%)">
      <h3>编辑资料</h3>
      <p style="margin-bottom:16px">修改昵称、简介与头像</p>
      <div style="display:flex;gap:14px;align-items:center;margin-bottom:16px">
        <div id="ed-avatar">${avatar(user, 'avatar-lg')}</div>
        <div>
          <button class="btn btn-sm" id="ed-avatar-btn">${icon('image')} 更换头像</button>
          <div class="hint" style="margin-top:6px">JPG / PNG / WebP，≤ 4MB</div>
          <input type="file" id="ed-avatar-file" accept="image/jpeg,image/png,image/webp" hidden>
        </div>
      </div>
      <div class="field">
        <label>昵称</label>
        <input class="input" id="ed-name" maxlength="24" value="${esc(user.display_name || user.username)}">
      </div>
      <div class="field">
        <label>简介</label>
        <textarea class="textarea" id="ed-bio" maxlength="200" style="min-height:88px">${esc(user.bio)}</textarea>
      </div>
      <div class="error-text" id="ed-err"></div>
      <div class="modal-actions">
        <button class="btn" id="ed-cancel">取消</button>
        <button class="btn btn-primary" id="ed-save">保存</button>
      </div>
    </div>`;
  document.getElementById('modal-root').appendChild(mask);

  const close = () => mask.remove();
  mask.addEventListener('click', (e) => { if (e.target === mask) close(); });
  mask.querySelector('#ed-cancel').onclick = close;

  mask.querySelector('#ed-avatar-btn').onclick = () => mask.querySelector('#ed-avatar-file').click();
  mask.querySelector('#ed-avatar-file').onchange = async (e) => {
    const f = e.target.files[0];
    if (!f) return;
    const fd = new FormData();
    fd.append('avatar', f, f.name);
    const csrf = document.cookie.match(/(?:^|; )lv_csrf=([^;]*)/)?.[1] || '';
    try {
      const res = await fetch('/api/auth/avatar', { method: 'POST', credentials: 'same-origin', headers: { 'X-CSRF-Token': decodeURIComponent(csrf) }, body: fd });
      const j = await res.json();
      if (!res.ok) throw new Error(j?.error?.message || '上传失败');
      mask.querySelector('#ed-avatar').innerHTML = `<img class="avatar-lg" src="${j.data.avatar_url}" alt="头像">`;
      await store.load();
      toast('头像已更新', 'ok');
    } catch (err) { mask.querySelector('#ed-err').textContent = err.message; }
  };

  mask.querySelector('#ed-save').onclick = async () => {
    const btn = mask.querySelector('#ed-save');
    btn.disabled = true;
    try {
      await api('/api/auth/profile', {
        method: 'PATCH',
        body: { display_name: mask.querySelector('#ed-name').value.trim(), bio: mask.querySelector('#ed-bio').value.trim() },
      });
      await store.load();
      toast('资料已保存', 'ok');
      close();
      done();
    } catch (e) {
      mask.querySelector('#ed-err').textContent = e.message;
      btn.disabled = false;
    }
  };
  void view;
}

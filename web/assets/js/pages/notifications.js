import { api } from '../api.js';
import { store } from '../store.js';
import { icon, esc, timeAgo, toast, emptyState, skeletonRows } from '../ui.js';
import { navigate } from '../router.js';

const KIND = {
  like: ['heart', '赞了你的视频'],
  favorite: ['star', '收藏了你的视频'],
  comment: ['comment', '评论了你的视频'],
  friend_request: ['friends', '请求添加你为好友'],
  friend_accept: ['friends', '接受了你的好友请求'],
  message: ['chat', '给你发了一条私信'],
};

export async function renderNotifications({ view }) {
  if (!store.user) { navigate('/login?next=/notifications'); return; }

  view.innerHTML = `
    <div class="page-narrow">
    <div class="section-head" style="margin-top:0">
      <h2>通知</h2>
      <button class="btn btn-sm" id="read-all">${icon('check')} 全部已读</button>
    </div>
    <div class="chips" style="margin-bottom:16px">
      <button class="chip active" data-filter="all">全部</button>
      <button class="chip" data-filter="unread">未读</button>
    </div>
    <div class="list-card" id="notif-list">${skeletonRows(5, 62)}</div>
    </div>`;

  const list = document.getElementById('notif-list');
  let filter = 'all';

  async function load() {
    list.innerHTML = skeletonRows(5, 62);
    try {
      const res = await api(`/api/notifications?limit=40${filter === 'unread' ? '&unread_only=true' : ''}`);
      const items = res.items || [];
      if (!items.length) {
        list.innerHTML = emptyState('没有通知', filter === 'unread' ? '所有通知都已读' : '有人互动时会出现在这里', 'bell');
        return;
      }
      list.innerHTML = items.map((n) => {
        const [ico, fallback] = KIND[n.kind] || ['info', n.kind];
        const href = n.video_id ? `/watch/${n.video_id}` : n.actor_id ? `/user/${n.actor_id}` : '#';
        return `<a class="notif ${esc(n.kind)} ${n.is_read ? '' : 'unread'}" href="${href}">
          <span class="ico">${icon(ico)}</span>
          <span class="grow">
            <span class="txt" style="display:block">${esc(n.content || fallback)}</span>
            <span class="tm">${timeAgo(n.created_at)}</span>
          </span>
        </a>`;
      }).join('');
      // 打开列表即标记已读（与后端行为一致）
      if (filter === 'all') {
        await api('/api/notifications/read', { method: 'POST', silent: true }).catch(() => {});
        store.patchStats({ unread_notifications: 0 });
      }
    } catch (e) {
      list.innerHTML = emptyState('加载失败', e.message, 'alert');
    }
  }

  document.querySelector('.chips').addEventListener('click', (e) => {
    const b = e.target.closest('[data-filter]');
    if (!b) return;
    filter = b.dataset.filter;
    document.querySelectorAll('[data-filter]').forEach((x) => x.classList.toggle('active', x === b));
    load();
  });

  document.getElementById('read-all').addEventListener('click', async () => {
    try {
      await api('/api/notifications/read', { method: 'POST' });
      store.patchStats({ unread_notifications: 0 });
      toast('已全部标记为已读', 'ok');
      load();
    } catch (e) { toast(e.message, 'err'); }
  });

  await load();
  return () => { document.title = '轻视频 · Light Video'; };
}

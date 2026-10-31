import { api } from '../api.js';
import { store } from '../store.js';
import { ws } from '../ws.js';
import { Player } from '../player.js';
import { DanmakuEngine } from '../danmaku.js';
import { icon, esc, fmtCount, fmtDuration, timeAgo, avatar, toast, confirmDialog, emptyState, skeletonRows } from '../ui.js';
import { sideItem } from '../cards.js';

const CATEGORY_NAME = {
  anime: '动画', game: '游戏', music: '音乐', tech: '科技',
  life: '生活', movie: '影视', study: '学习', other: '其他',
};

// 模块级去重：即使某次渲染的清理没跑到，也不会把同一条广播重复上屏
const seenDanmaku = new Set();

export async function renderWatch({ view, params, query }) {
  const id = Number(params.id);
  if (!id) { view.innerHTML = emptyState('视频不存在', '', 'alert'); return; }

  view.innerHTML = `
    <div class="watch-layout">
      <div class="watch-main">
        <div id="player-mount">
          <div class="player"><div class="spinner-overlay" style="opacity:1"><div class="spinner"></div></div></div>
        </div>
        <h1 class="watch-title" id="v-title">
          <span class="skeleton" style="display:inline-block;width:60%;height:30px"></span>
        </h1>
        <div class="watch-sub" id="v-sub"></div>
        <div class="watch-actions" id="v-actions"></div>
        <div id="v-author"></div>
        <div id="v-desc"></div>

        <div class="panel-tabs" id="panel-tabs">
          <button class="panel-tab active" data-tab="comments">评论 <span class="n" id="c-count">0</span></button>
          <button class="panel-tab" data-tab="danmaku">弹幕 <span class="n" id="d-count">0</span></button>
        </div>
        <div id="tab-comments">
          <div class="comment-form" id="comment-form"></div>
          <div id="comments">${skeletonRows(2, 84)}</div>
        </div>
        <div id="tab-danmaku" hidden>
          <div class="dm-list" id="dm-list"><div class="empty">加载中…</div></div>
        </div>
      </div>

      <aside class="watch-side">
        <div class="side-title"><span>随便看看</span>${icon('sparkles')}</div>
        <div class="side-list" id="related"></div>
      </aside>
    </div>`;

  let data;
  try {
    data = await api(`/api/videos/${id}`);
  } catch (e) {
    document.getElementById('player-mount').innerHTML = emptyState('无法播放', e.message, 'alert');
    document.getElementById('v-title').textContent = '';
    return;
  }

  // ==================== 基本信息 ====================
  document.title = `${data.title} · 轻视频`;
  document.getElementById('v-title').textContent = data.title;
  document.getElementById('v-sub').innerHTML = `
    <span class="stat">${icon('eye')}${fmtCount(data.views)} 播放</span>
    <span class="stat">${icon('danmaku')}${fmtCount(data.danmaku_count)} 弹幕</span>
    <span class="stat">${icon('clock')}${timeAgo(data.created_at)}</span>
    <span class="stat" id="presence">${icon('wifi')}—</span>
    ${data.status === 'processing' ? '<span class="tag">转码中</span>' : ''}
    ${data.visibility !== 'public' ? `<span class="tag">${data.visibility === 'private' ? '私密' : '未列出'}</span>` : ''}`;

  const actions = document.getElementById('v-actions');
  actions.innerHTML = `
    <button class="act like ${data.liked ? 'on' : ''}" data-act="like">
      ${icon('heart')}<span>点赞</span><b data-like-count>${fmtCount(data.likes_count)}</b>
    </button>
    <button class="act fav ${data.favorited ? 'on' : ''}" data-act="fav">
      ${icon('star')}<span>收藏</span><b data-fav-count>${fmtCount(data.favorites_count ?? 0)}</b>
    </button>
    <button class="act" data-act="share">${icon('share')}<span>分享</span></button>
    <button class="act" data-act="theater">${icon('video')}<span>影院模式</span></button>
    ${data.is_owner ? `<a class="act" href="/user/${data.author_id}">${icon('settings')}<span>管理</span></a>` : ''}
    <span style="flex:1"></span>
    <span style="font-size:12px;color:var(--text-mute)">${[
      data.duration_ms ? fmtDuration(data.duration_ms) : '',
      data.width && data.height ? `${data.width}×${data.height}` : '',
      data.renditions?.length ? `${data.renditions.length + 1} 档清晰度` : '',
    ].filter(Boolean).join(' · ')}</span>`;

  // 作者卡片
  const authorEl = document.getElementById('v-author');
  authorEl.innerHTML = `
    <div class="author-row">
      <a href="/user/${data.author_id}">${avatar({ username: data.author_name, avatar_url: data.author_avatar }, 'avatar')}</a>
      <div class="grow">
        <a class="name" href="/user/${data.author_id}">${esc(data.author_name)}</a>
        <div class="sub">UP主 · ${fmtCount(data.views)} 次播放 · 发布于 ${timeAgo(data.created_at)}</div>
      </div>
      ${data.is_owner ? '' : `<button class="btn btn-sm ${data.following ? '' : 'btn-primary'}" data-follow>${data.following ? '已是好友' : '+ 加好友'}</button>`}
      ${data.is_owner ? '' : `<a class="btn btn-sm" href="/chat/${data.author_id}">${icon('chat')} 私信</a>`}
    </div>`;

  // 简介
  const descEl = document.getElementById('v-desc');
  descEl.innerHTML = `
    <div class="desc-box ${data.description.length > 120 ? 'collapsed' : ''}" id="desc-box">${esc(data.description) || '<span style="color:var(--text-mute)">这个视频没有简介</span>'}</div>
    ${data.description.length > 120 ? '<button class="desc-toggle" id="desc-toggle">展开简介</button>' : ''}
    ${data.tags?.length ? `<div class="tags">${data.tags.map((t) => `<span class="tag">${esc(t)}</span>`).join('')}</div>` : ''}`;
  document.getElementById('desc-toggle')?.addEventListener('click', (e) => {
    const box = document.getElementById('desc-box');
    const collapsed = box.classList.toggle('collapsed');
    e.currentTarget.textContent = collapsed ? '展开简介' : '收起简介';
  });

  // ==================== 播放器 + 弹幕 ====================
  const engine = new DanmakuEngine();
  const renditions = (data.renditions || []).map((r) => ({ name: r.name, url: r.url, height: r.height }));
  const player = new Player(document.getElementById('player-mount'), {
    src: data.stream_url,
    poster: data.poster_url,
    height: data.height,
    renditions,
    hlsUrl: data.hls_url,
    danmaku: engine,
  });

  // 弹幕加载
  api(`/api/videos/${id}/danmaku`).then((list) => {
    engine.setItems(list || []);
    document.getElementById('d-count').textContent = fmtCount((list || []).length);
    renderDanmakuList(list || []);
  }).catch(() => {});

  // 分享链接自动播放：等 canplay 再触发，避免在 load() 期间调用被拒绝
  if (query.autoplay === '1') {
    const v = player.video;
    const fire = () => player.play().catch(() => {});
    if (v.readyState >= 3) fire();
    else v.addEventListener('canplay', fire, { once: true });
  }

  // 断点续播
  api(`/api/videos/${id}/progress`, { silent: true }).then((p) => {
    const pos = (p?.position_ms || 0) / 1000;
    const dur = data.duration_ms / 1000;
    if (pos > 8 && (!dur || pos < dur - 6)) {
      const bar = document.createElement('div');
      bar.className = 'toast ok';
      bar.style.cssText = 'position:absolute;z-index:14;left:16px;bottom:86px;background:rgba(16,18,26,.92);color:#fff';
      bar.innerHTML = `${icon('clock')}<span>已为你定位到 ${fmtDuration(pos * 1000)}</span><button class="btn btn-sm" style="margin-left:6px">从头播放</button>`;
      document.querySelector('.player')?.appendChild(bar);
      player.seek(pos);
      bar.querySelector('button').onclick = () => { player.seek(0); bar.remove(); };
      setTimeout(() => bar.remove(), 9000);
    }
  }).catch(() => {});

  // 发送弹幕
  const onDanmaku = (e) => {
    if (!store.user) {
      e.preventDefault();
      toast('登录后才能发送弹幕', 'err');
      return;
    }
    // 由本页统一负责发送：本地不回显，等 WS 广播回来只加一次
    e.preventDefault();
    const d = e.detail;
    if (ws.connected) {
      ws.danmaku({ video_id: id, content: d.content, time_ms: d.time_ms, color: d.color, mode: d.mode, font_size: d.font_size });
    } else {
      api(`/api/videos/${id}/danmaku`, { method: 'POST', body: d })
        .then((row) => { engine.addLocal(row); pushDmRow(row); })
        .catch((err) => toast(err.message, 'err'));
    }
  };
  player.addEventListener('danmaku', onDanmaku);

  // 弹幕开关状态
  const dmBtn = document.querySelector('[data-act=dm]');
  if (dmBtn) dmBtn.style.opacity = engine.config.enabled ? '1' : '.45';

  // ESC 退出影院模式
  const onCinemaKey = (e) => {
    if (e.key === 'Escape' && document.body.classList.contains('cinema')) {
      document.body.classList.remove('cinema');
      player.setTheater(false);
      actions.querySelector('[data-act=theater]')?.classList.remove('on');
    }
  };
  document.addEventListener('keydown', onCinemaKey);

  // 实时弹幕
  const offDanmaku = ws.on('danmaku', (msg) => {
    const d = msg.data;
    if (!d || d.id == null || seenDanmaku.has(d.id)) return;
    seenDanmaku.add(d.id);
    if (seenDanmaku.size > 4000) seenDanmaku.clear();
    engine.addLocal(d);
    pushDmRow(d);
    const c = document.getElementById('d-count');
    if (c) c.textContent = fmtCount((Number(c.textContent.replace(/[^\d]/g, '')) || 0) + 1);
  });
  const offPresence = ws.on('presence', (msg) => {
    if (msg.room !== `video:${id}`) return;
    const el = document.getElementById('presence');
    if (el) el.innerHTML = `${icon('wifi')}${msg.count} 人正在看`;
  });
  ws.joinVideo(id);

  // 进度上报
  const saveProgress = () => {
    if (!store.user) return;
    const pos = Math.round((player.video.currentTime || 0) * 1000);
    if (pos > 0) api(`/api/videos/${id}/progress`, { method: 'POST', body: { position_ms: pos }, silent: true }).catch(() => {});
  };
  const progressTimer = setInterval(saveProgress, 15000);
  const onPauseSave = () => saveProgress();
  player.video.addEventListener('pause', onPauseSave);
  window.addEventListener('beforeunload', saveProgress);

  // ==================== 交互 ====================
  actions.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-act]');
    if (!btn) return;
    const act = btn.dataset.act;

    if (act === 'like' || act === 'fav') {
      if (!store.user) { toast('请先登录', 'err'); return; }
      try {
        const r = await api(`/api/videos/${id}/${act === 'like' ? 'like' : 'favorite'}`, { method: 'POST' });
        if (act === 'like') {
          btn.classList.toggle('on', r.liked);
          btn.querySelector('[data-like-count]').textContent = fmtCount(r.likes_count);
        } else {
          btn.classList.toggle('on', r.favorited);
          btn.querySelector('[data-fav-count]').textContent = fmtCount(r.favorites_count);
        }
      } catch (err) { toast(err.message, 'err'); }
    }

    if (act === 'share') {
      const url = `${location.origin}/watch/${id}`;
      try {
        if (navigator.share) await navigator.share({ title: data.title, url });
        else { await navigator.clipboard.writeText(url); toast('链接已复制', 'ok'); }
      } catch { /* 用户取消 */ }
    }

    if (act === 'theater') {
      const on = !document.body.classList.contains('cinema');
      player.setTheater(on);
      btn.classList.toggle('on', on);
      document.body.classList.toggle('cinema', on);
      if (!on) window.scrollTo({ top: 0, behavior: 'smooth' });
    }
  });

  authorEl.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-follow]');
    if (!btn) return;
    if (!store.user) { toast('请先登录', 'err'); return; }
    try {
      const r = await api(`/api/users/${data.author_id}/friend-request`, { method: 'POST' });
      if (r.status === 'accepted') { btn.textContent = '已是好友'; btn.classList.remove('btn-primary'); }
      else { btn.textContent = '请求已发送'; btn.classList.remove('btn-primary'); }
      toast('操作成功', 'ok');
    } catch (err) { toast(err.message, 'err'); }
  });

  // 标签页
  const tabs = document.getElementById('panel-tabs');
  tabs.addEventListener('click', (e) => {
    const b = e.target.closest('[data-tab]');
    if (!b) return;
    const tab = b.dataset.tab;
    tabs.querySelectorAll('.panel-tab').forEach((t) => t.classList.toggle('active', t === b));
    document.getElementById('tab-comments').hidden = tab !== 'comments';
    document.getElementById('tab-danmaku').hidden = tab !== 'danmaku';
  });

  // ==================== 评论 ====================
  renderCommentForm(id);
  loadComments(id, data);

  // ==================== 相关推荐 ====================
  api(`/api/videos/${id}/related`, { silent: true }).then((list) => {
    const el = document.getElementById('related');
    el.innerHTML = (list || []).length
      ? list.map(sideItem).join('')
      : '<div style="color:var(--text-mute);font-size:13px">暂无推荐</div>';
  }).catch(() => {
    document.getElementById('related').innerHTML = '<div style="color:var(--text-mute);font-size:13px">加载失败</div>';
  });

  function pushDmRow(d) {
    const list = document.getElementById('dm-list');
    if (!list) return;
    const empty = list.querySelector('.empty');
    if (empty) empty.remove();
    const row = document.createElement('div');
    row.className = 'dm-item';
    row.dataset.id = d.id;
    row.innerHTML = `<span class="t">${fmtDuration(d.time_ms)}</span>
      <span class="txt" style="color:${esc(d.color || '#fff')}">${esc(d.content)}</span>
      <span class="who">${esc(d.username || '')}</span>
      ${store.user ? '<button data-del>删除</button>' : ''}`;
    row.querySelector('.txt').addEventListener('click', () => player.seek((d.time_ms || 0) / 1000));
    row.querySelector('[data-del]')?.addEventListener('click', async () => {
      if (!await confirmDialog({ title: '删除弹幕', message: '确定删除这条弹幕吗？', danger: true, confirmText: '删除' })) return;
      try {
        await api(`/api/danmaku/${d.id}`, { method: 'DELETE' });
        row.remove();
        toast('已删除', 'ok');
      } catch (err) { toast(err.message, 'err'); }
    });
    list.prepend(row);
  }

  function renderDanmakuList(list) {
    const el = document.getElementById('dm-list');
    if (!list.length) { el.innerHTML = emptyState('还没有弹幕', '抢占第一条弹幕吧', 'danmaku'); return; }
    el.innerHTML = list.slice().reverse().map((d) => `
      <div class="dm-item" data-t="${d.time_ms}">
        <span class="t">${fmtDuration(d.time_ms)}</span>
        <span class="txt" style="color:${esc(d.color || '#fff')}">${esc(d.content)}</span>
        <span class="who">${esc(d.username || '')}</span>
      </div>`).join('');
    el.querySelectorAll('.dm-item').forEach((row) => {
      row.addEventListener('click', () => player.seek(Number(row.dataset.t) / 1000));
    });
  }

  return () => {
    clearInterval(progressTimer);
    window.removeEventListener('beforeunload', saveProgress);
    player.video.removeEventListener('pause', onPauseSave);
    player.removeEventListener('danmaku', onDanmaku);
    offDanmaku();
    offPresence();
    document.removeEventListener('keydown', onCinemaKey);
    ws.leaveVideo(id);
    saveProgress();
    player.destroy();
    document.body.classList.remove('cinema');
    document.title = '轻视频 · Light Video';
  };
}

// ==================== 评论 ====================

function renderCommentForm(videoId) {
  const el = document.getElementById('comment-form');
  if (!store.user) {
    el.innerHTML = `<div class="card" style="padding:16px;width:100%;display:flex;align-items:center;gap:12px">
      <span style="color:var(--text-dim);font-size:14px">登录后参与评论</span>
      <a class="btn btn-sm btn-primary" href="/login">登录</a>
    </div>`;
    return;
  }
  el.innerHTML = `
    ${avatar(store.user)}
    <div class="grow">
      <textarea class="textarea" id="c-input" maxlength="1000" placeholder="说点什么…（Ctrl + Enter 发送）"></textarea>
      <div class="cf-actions"><span class="hint" id="c-hint"></span><button class="btn btn-primary btn-sm" id="c-send">发送</button></div>
    </div>`;
  const input = el.querySelector('#c-input');
  const send = async () => {
    const content = input.value.trim();
    if (!content) return;
    try {
      const row = await api(`/api/videos/${videoId}/comments`, { method: 'POST', body: { content } });
      input.value = '';
      prependComment(row);
      const cc = document.getElementById('c-count');
      cc.textContent = String((Number(cc.textContent) || 0) + 1);
      toast('评论已发布', 'ok');
    } catch (e) { toast(e.message, 'err'); }
  };
  el.querySelector('#c-send').addEventListener('click', send);
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); send(); }
  });
}

async function loadComments(videoId, video) {
  const el = document.getElementById('comments');
  try {
    const res = await api(`/api/videos/${videoId}/comments?limit=30`);
    document.getElementById('c-count').textContent = String(res.total ?? res.items?.length ?? 0);
    el.innerHTML = (res.items || []).length
      ? res.items.map(commentRow).join('')
      : emptyState('还没有评论', '来抢沙发吧', 'comment');
    bindCommentActions(el);
  } catch (e) {
    el.innerHTML = emptyState('评论加载失败', e.message, 'alert');
  }
  void video;
}

function commentRow(c) {
  return `
    <div class="comment" data-id="${c.id}">
      ${avatar({ username: c.username, avatar_url: c.avatar_url })}
      <div class="c-body">
        <div class="c-head"><span class="c-name">${esc(c.username)}</span><span>${timeAgo(c.created_at)}</span></div>
        <div class="c-text">${esc(c.content)}</div>
        <div class="c-actions">
          ${c.is_owner ? '<button data-del>删除</button>' : ''}
        </div>
      </div>
    </div>`;
}

function prependComment(c) {
  const el = document.getElementById('comments');
  const empty = el.querySelector('.empty');
  if (empty) empty.remove();
  el.insertAdjacentHTML('afterbegin', commentRow(c));
  bindCommentActions(el);
}

function bindCommentActions(root) {
  root.querySelectorAll('.comment').forEach((node) => {
    const del = node.querySelector('[data-del]');
    if (!del || del.dataset.bound) return;
    del.dataset.bound = '1';
    del.addEventListener('click', async () => {
      if (!await confirmDialog({ title: '删除评论', message: '删除后不可恢复，确定继续？', danger: true, confirmText: '删除' })) return;
      try {
        await api(`/api/comments/${node.dataset.id}`, { method: 'DELETE' });
        node.remove();
        toast('已删除', 'ok');
      } catch (e) { toast(e.message, 'err'); }
    });
  });
}

export { CATEGORY_NAME };

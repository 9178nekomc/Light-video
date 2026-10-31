import { api } from '../api.js';
import { store } from '../store.js';
import { icon, esc, fmtCount, skeletonGrid, emptyState } from '../ui.js';
import { videoCard } from '../cards.js';

const CATEGORIES = [
  ['all', '全部'], ['anime', '动画'], ['game', '游戏'], ['music', '音乐'],
  ['tech', '科技'], ['life', '生活'], ['movie', '影视'], ['study', '学习'],
];

const state = { feed: 'latest', category: 'all', q: '', page: 1, hasMore: true, loading: false, q_items: [] };

export async function renderHome({ view, query }) {
  state.feed = query.feed || 'latest';
  state.category = query.category || 'all';
  state.q = query.q || '';
  state.page = 1;
  state.hasMore = true;
  state.loading = false;

  view.innerHTML = `
    <section class="hero" id="hero">
      <div>
        <h1>${state.q ? `搜索 <span>${esc(state.q)}</span>` : '把喜欢的内容，<span>分享给同频的人</span>'}</h1>
        <p>${state.q ? '从全部视频中匹配标题与简介' : 'Rust 驱动的高性能视频社区 · 分片上传 · 多分辨率 · 实时弹幕与私信'}</p>
        <div class="hero-actions">
          <a class="btn btn-primary btn-lg" href="/upload">${icon('upload')} 开始投稿</a>
          <button class="btn btn-lg" id="hero-random">${icon('sparkles')} 随便看看</button>
        </div>
        <div class="hero-badges" id="hero-stats">
          <span><b>—</b> 视频</span><span><b>—</b> 创作者</span><span><b>—</b> 弹幕</span>
        </div>
      </div>
    </section>

    ${state.q ? '' : `
    <div class="section-head" style="margin-top:0">
      <h2>发现</h2>
      <div class="chips" id="feeds">
        ${[['latest', '最新'], ['hot', '热门'], ...(store.user ? [['following', '关注']] : [])]
          .map(([id, name]) => `<button class="chip ${state.feed === id ? 'active' : ''}" data-feed="${id}">${name}</button>`).join('')}
      </div>
    </div>`}

    <div class="chips" id="cats" style="margin-bottom:20px">
      ${CATEGORIES.map(([id, name]) => `<button class="chip ${state.category === id ? 'active' : ''}" data-cat="${id}">${name}</button>`).join('')}
    </div>

    <div id="feed">${skeletonGrid(12)}</div>
    <div id="feed-more" style="height:40px"></div>`;

  if (state.q) {
    document.getElementById('hero')?.remove();
    view.insertAdjacentHTML('afterbegin', `
      <div class="section-head" style="margin-top:0">
        <h2>搜索结果</h2>
        <a class="more" href="/">清除搜索 ${icon('close')}</a>
      </div>`);
  }

  api('/api/stats', { silent: true }).then((s) => {
    const el = document.getElementById('hero-stats');
    if (el && s) {
      el.innerHTML = `<span><b>${fmtCount(s.videos)}</b> 视频</span>
        <span><b>${fmtCount(s.users)}</b> 创作者</span>
        <span><b>${fmtCount(s.danmaku)}</b> 弹幕</span>`;
    }
  }).catch(() => {});

  document.getElementById('hero-random')?.addEventListener('click', async () => {
    try {
      const res = await api('/api/videos?feed=hot&limit=1&page=' + (1 + Math.floor(Math.random() * 5)));
      const item = res.items?.[0];
      if (item) location.href = `/watch/${item.id}`;
    } catch { /* ignore */ }
  });

  view.querySelector('#cats').addEventListener('click', (e) => {
    const b = e.target.closest('[data-cat]');
    if (!b) return;
    state.category = b.dataset.cat;
    view.querySelectorAll('#cats .chip').forEach((c) => c.classList.toggle('active', c.dataset.cat === state.category));
    reload();
  });

  view.querySelector('#feeds')?.addEventListener('click', (e) => {
    const b = e.target.closest('[data-feed]');
    if (!b) return;
    state.feed = b.dataset.feed;
    view.querySelectorAll('#feeds .chip').forEach((c) => c.classList.toggle('active', c.dataset.feed === state.feed));
    reload();
  });

  const io = new IntersectionObserver((entries) => {
    if (entries[0].isIntersecting) loadMore();
  }, { rootMargin: '600px' });
  io.observe(document.getElementById('feed-more'));

  await reload();
  return () => io.disconnect();
}

async function reload() {
  state.page = 1;
  state.hasMore = true;
  document.getElementById('feed').innerHTML = skeletonGrid(8);
  await loadMore(true);
}

/** 取（或重建）卡片栅格容器，保证卡片始终在 .grid 内 */
function gridEl(reset) {
  const feed = document.getElementById('feed');
  if (reset) {
    feed.innerHTML = '<div class="grid"></div>';
  }
  let grid = feed.querySelector('.grid');
  if (!grid) {
    grid = document.createElement('div');
    grid.className = 'grid';
    feed.prepend(grid);
  }
  return grid;
}

// 请求代次：切换分区/搜索时让旧请求的响应作废，避免骨架屏卡死或重复追加
let generation = 0;

async function loadMore(reset = false) {
  // reset 必须能抢占正在进行的请求，否则会留下永远不消失的骨架卡片
  if (!reset && (state.loading || !state.hasMore)) return;
  const my = ++generation;
  state.loading = true;
  const feedEl = document.getElementById('feed');
  try {
    const params = new URLSearchParams({ page: String(state.page), limit: '24', feed: state.feed });
    if (state.category !== 'all') params.set('category', state.category);
    if (state.q) params.set('q', state.q);
    const res = await api(`/api/videos?${params}`);
    if (my !== generation) return; // 过期响应，丢弃
    const items = res.items || [];
    const grid = gridEl(reset);
    if (!items.length) {
      if (reset) {
        feedEl.innerHTML = state.feed === 'following'
          ? emptyState('关注列表还很安静', '去关注一些创作者，这里就会出现他们的新作品', 'users')
          : state.q ? emptyState('没有找到相关视频', '换个关键词试试', 'search') : emptyState('还没有视频', '成为第一个上传的人吧', 'video');
      }
      state.hasMore = false;
      return;
    }
    grid.insertAdjacentHTML('beforeend', items.map(videoCard).join(''));
    state.page += 1;
    state.hasMore = !!res.has_more;
    if (!state.hasMore && state.page > 1) {
      feedEl.insertAdjacentHTML('beforeend', `<div style="text-align:center;color:var(--text-mute);font-size:12.5px;padding:22px 0">— 已经到底了 —</div>`);
    }
  } catch (e) {
    if (my === generation && reset) feedEl.innerHTML = emptyState('加载失败', e.message, 'alert');
  } finally {
    if (my === generation) state.loading = false;
  }
}

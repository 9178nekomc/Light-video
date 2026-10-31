import { icon, esc, fmtCount, fmtDuration, avatar } from './ui.js';

export function poster(v) {
  if (v.poster_url) return `<img src="${esc(v.poster_url)}" alt="${esc(v.title)}" loading="lazy" decoding="async">`;
  const name = String(v.title || '轻视频');
  const ch = [...name][0] || '轻';
  return `<div class="poster-grad">${esc(ch)}</div>`;
}

export function videoCard(v) {
  return `
    <a class="vcard" href="/watch/${v.id}" title="${esc(v.title)}">
      <div class="vcard-media">
        ${poster(v)}
        <div class="vcard-play"><span>${icon('play')}</span></div>
        ${v.duration_ms ? `<span class="duration">${fmtDuration(v.duration_ms)}</span>` : ''}
      </div>
      <div class="vcard-title">${esc(v.title)}</div>
      <div class="vcard-meta">
        <span class="stat">${icon('eye')}${fmtCount(v.views)}</span>
        <span class="dot">·</span>
        <span class="stat hide-sm">${icon('danmaku')}${fmtCount(v.danmaku_count)}</span>
        <span class="dot hide-sm">·</span>
        <span>${esc(v.author_name || '匿名')}</span>
      </div>
    </a>`;
}

export function sideItem(v) {
  return `
    <a class="side-item" href="/watch/${v.id}">
      <div class="thumb">${poster(v)}${v.duration_ms ? `<span class="duration">${fmtDuration(v.duration_ms)}</span>` : ''}</div>
      <div>
        <div class="t">${esc(v.title)}</div>
        <div class="m">${esc(v.author_name || '')}</div>
        <div class="m">${fmtCount(v.views)} 播放 · ${fmtCount(v.danmaku_count)} 弹幕</div>
      </div>
    </a>`;
}

export function userRow(u, actions = '') {
  return `
    <div class="list-row">
      <a href="/user/${u.id}">${avatar(u)}</a>
      <div class="grow">
        <a class="nm" href="/user/${u.id}">
          ${esc(u.display_name || u.username)}
          ${u.online ? '<span class="online-dot" title="在线"></span>' : ''}
        </a>
        <div class="sb">@${esc(u.username)}${u.bio ? ` · ${esc(u.bio)}` : ''}</div>
      </div>
      ${actions}
    </div>`;
}

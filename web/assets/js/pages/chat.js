import { api } from '../api.js';
import { store } from '../store.js';
import { ws } from '../ws.js';
import { icon, esc, avatar, timeAgo, toast, emptyState, debounce, skeletonRows } from '../ui.js';
import { navigate } from '../router.js';

export async function renderChat({ view, params }) {
  if (!store.user) { navigate('/login?next=/chat'); return; }
  const peerId = params.id ? Number(params.id) : 0;

  view.innerHTML = `
    <div class="chat-layout ${peerId ? 'show-thread' : ''}" id="chat-layout">
      <aside class="chat-side">
        <div class="chat-side-head">
          <h2>私信</h2>
          <a class="icon-btn" href="/friends" title="好友">${icon('friends')}</a>
        </div>
        <div class="conv-list" id="convs">${skeletonRows(4)}</div>
      </aside>
      <section class="chat-main" id="chat-main">
        ${peerId ? '' : `<div class="empty" style="margin:auto">${icon('chat')}<strong>选择一个会话开始聊天</strong><span>只有好友之间才能互发私信</span><a class="btn btn-primary btn-sm" href="/friends">去找好友</a></div>`}
      </section>
    </div>`;

  const convsEl = document.getElementById('convs');
  let peer = null;
  let lastId = 0;
  let typingTimer = 0;

  async function loadConversations() {
    try {
      const rows = await api('/api/conversations');
      if (!rows.length) {
        convsEl.innerHTML = emptyState('还没有会话', '去和好友聊聊吧', 'chat');
        return;
      }
      convsEl.innerHTML = rows.map((c) => `
        <div class="conv ${c.user_id === peerId ? 'active' : ''}" data-uid="${c.user_id}">
          ${avatar({ username: c.username, avatar_url: c.avatar_url })}
          <div class="grow">
            <div class="nm"><span>${esc(c.username)}</span><span>${timeAgo(c.last_time)}</span></div>
            <div class="last">${esc(c.last_content)}</div>
          </div>
          ${c.unread ? `<span class="unread">${c.unread > 99 ? '99+' : c.unread}</span>` : ''}
        </div>`).join('');
    } catch (e) {
      convsEl.innerHTML = emptyState('加载失败', e.message, 'alert');
    }
  }

  convsEl.addEventListener('click', (e) => {
    const row = e.target.closest('[data-uid]');
    if (row) navigate(`/chat/${row.dataset.uid}`);
  });

  if (!peerId) {
    await loadConversations();
    return;
  }

  // ==================== 会话主体 ====================
  const main = document.getElementById('chat-main');
  main.innerHTML = `
    <div class="chat-head">
      <a class="icon-btn" href="/chat" data-no-spa style="display:none" id="back">${icon('chevronLeft')}</a>
      <div id="peer-avatar"></div>
      <div class="grow">
        <div class="nm" id="peer-name">加载中…</div>
        <div class="st" id="peer-status"></div>
      </div>
      <a class="icon-btn" id="peer-home" title="主页">${icon('user')}</a>
    </div>
    <div class="chat-body" id="body"><div class="empty" style="margin:auto">加载消息中…</div></div>
    <div class="typing" id="typing"></div>
    <div class="chat-compose">
      <textarea id="msg-input" rows="1" maxlength="2000" placeholder="输入消息，Enter 发送，Shift+Enter 换行"></textarea>
      <button class="btn btn-primary" id="msg-send">${icon('send')}</button>
    </div>`;

  const body = document.getElementById('body');
  const input = document.getElementById('msg-input');
  document.getElementById('back').style.display = '';

  try {
    peer = await api(`/api/users/${peerId}`);
    document.getElementById('peer-avatar').innerHTML = avatar(peer.user);
    document.getElementById('peer-name').textContent = peer.user.display_name || peer.user.username;
    document.getElementById('peer-status').innerHTML = peer.online
      ? '<span class="online-dot" style="display:inline-block;margin-right:5px"></span>在线'
      : '离线';
    document.getElementById('peer-home').href = `/user/${peerId}`;
    if (!peer.relation.is_friend) {
      body.innerHTML = emptyState('你们还不是好友', '加个好友就能开始聊天了', 'users');
      input.disabled = true;
      document.getElementById('msg-send').disabled = true;
    }
  } catch {
    body.innerHTML = emptyState('用户不存在', '', 'alert');
  }

  function msgHtml(m) {
    const mine = m.sender_id === store.user.id;
    const share = m.kind === 'video' && m.video_id
      ? `<a class="video-share" href="/watch/${m.video_id}">
           ${icon('video')}<span style="font-size:12.5px">查看分享的视频</span>
         </a>` : '';
    return `<div class="msg ${mine ? 'me' : ''}" data-id="${m.id}">
      <div>
        <div class="bubble">${esc(m.content)}${share}</div>
        <div class="time">${timeAgo(m.created_at)}</div>
      </div>
    </div>`;
  }

  async function loadMessages() {
    try {
      const res = await api(`/api/messages/${peerId}?limit=60`);
      const items = res.items || [];
      if (!items.length) {
        body.innerHTML = emptyState('开始聊天吧', '发一条消息打个招呼', 'chat');
        return;
      }
      body.innerHTML = items.map(msgHtml).join('');
      lastId = items[items.length - 1].id;
      scrollBottom();
      store.patchStats({ unread_messages: Math.max(0, (store.stats.unread_messages || 0)) });
    } catch (e) {
      body.innerHTML = emptyState('消息加载失败', e.message, 'alert');
    }
  }

  function scrollBottom() {
    body.scrollTop = body.scrollHeight;
  }

  function append(m) {
    const empty = body.querySelector('.empty');
    if (empty) empty.remove();
    body.insertAdjacentHTML('beforeend', msgHtml(m));
    scrollBottom();
    if (m.id) lastId = Math.max(lastId, m.id);
  }

  async function send() {
    const content = input.value.trim();
    if (!content || input.disabled) return;
    input.value = '';
    input.style.height = 'auto';
    const clientId = `c-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const temp = {
      id: 0, sender_id: store.user.id, receiver_id: peerId,
      content, kind: 'text', created_at: new Date().toISOString(), client_id: clientId,
    };
    append(temp);

    const node = body.querySelector(`.msg[data-id="0"]:last-child`);
    if (ws.connected) {
      ws.chat(peerId, content, clientId, null);
      node?.setAttribute('data-client', clientId);
    } else {
      try {
        const m = await api('/api/messages', { method: 'POST', body: { to: peerId, content } });
        node?.setAttribute('data-id', m.id);
        if (node) node.querySelector('.time').textContent = timeAgo(m.created_at);
      } catch (e) {
        toast(e.message, 'err');
        node?.remove();
      }
    }
    loadConversations();
  }

  document.getElementById('msg-send').addEventListener('click', send);
  input.addEventListener('keydown', (e) => {
    e.stopPropagation();
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
  });
  input.addEventListener('input', () => {
    input.style.height = 'auto';
    input.style.height = `${Math.min(140, input.scrollHeight)}px`;
    if (Date.now() - typingTimer > 2500) {
      typingTimer = Date.now();
      ws.typing(peerId);
    }
  });

  const offChat = ws.on('chat', (msg) => {
    const m = msg.data;
    if (!m) return;
    if (m.sender_id === peerId) {
      append({ ...m, created_at: m.created_at });
      if (document.visibilityState === 'visible') api('/api/messages/read', { method: 'POST', body: { user_id: peerId } }).catch(() => {});
      document.getElementById('typing').textContent = '';
      loadConversations();
    }
  });

  const offAck = ws.on('chat_ack', (msg) => {
    const node = body.querySelector(`.msg[data-client="${msg.client_id}"]`);
    if (node) {
      node.dataset.id = msg.id;
      node.querySelector('.time').textContent = timeAgo(msg.created_at);
      node.removeAttribute('data-client');
    }
  });

  const offTyping = ws.on('typing', (msg) => {
    if (msg.from !== peerId) return;
    const el = document.getElementById('typing');
    el.textContent = `${msg.username} 正在输入…`;
    clearTimeout(el._t);
    el._t = setTimeout(() => { el.textContent = ''; }, 2600);
  });

  await Promise.all([loadMessages(), loadConversations()]);
  input.focus();

  return () => {
    offChat(); offAck(); offTyping();
    document.title = '轻视频 · Light Video';
  };
}

const _ = debounce;
void _;

// WebSocket 全双工客户端：自动重连 + 指数退避 + 事件总线
import { store } from './store.js';

class WSClient {
  constructor() {
    this.socket = null;
    this.handlers = new Map();
    this.stateHandlers = new Set();
    this.attempt = 0;
    this.closedByUser = false;
    this.pending = [];
    this.pingTimer = null;
  }

  url() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${proto}//${location.host}/ws`;
  }

  connect() {
    if (this.socket && (this.socket.readyState === WebSocket.OPEN || this.socket.readyState === WebSocket.CONNECTING)) return;
    this.closedByUser = false;
    let ws;
    try {
      ws = new WebSocket(this.url());
    } catch {
      this.scheduleReconnect();
      return;
    }
    this.socket = ws;

    ws.onopen = () => {
      this.attempt = 0;
      this.emitState('open');
      while (this.pending.length) {
        const msg = this.pending.shift();
        try { ws.send(JSON.stringify(msg)); } catch { /* ignore */ }
      }
      clearInterval(this.pingTimer);
      this.pingTimer = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) this.send({ type: 'ping', t: Date.now() });
      }, 25000);
    };

    ws.onmessage = (ev) => {
      let msg;
      try { msg = JSON.parse(ev.data); } catch { return; }
      if (!msg || !msg.type) return;
      if (msg.type === 'hello') {
        this.user = msg.user;
        this.emitState('hello');
      }
      const list = this.handlers.get(msg.type);
      if (list) list.forEach((fn) => { try { fn(msg); } catch (e) { console.error(e); } });
      const any = this.handlers.get('*');
      if (any) any.forEach((fn) => { try { fn(msg); } catch (e) { console.error(e); } });
    };

    ws.onclose = () => {
      clearInterval(this.pingTimer);
      this.emitState('closed');
      if (!this.closedByUser) this.scheduleReconnect();
    };
    ws.onerror = () => {
      try { ws.close(); } catch { /* ignore */ }
    };
  }

  scheduleReconnect() {
    this.attempt = Math.min(this.attempt + 1, 6);
    const delay = Math.min(12000, 600 * 2 ** this.attempt) + Math.random() * 300;
    clearTimeout(this.retryTimer);
    this.retryTimer = setTimeout(() => this.connect(), delay);
  }

  send(obj) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(obj));
      return true;
    }
    this.pending.push(obj);
    if (!this.socket || this.socket.readyState === WebSocket.CLOSED) this.connect();
    return false;
  }

  on(type, fn) {
    if (!this.handlers.has(type)) this.handlers.set(type, new Set());
    this.handlers.get(type).add(fn);
    return () => this.handlers.get(type)?.delete(fn);
  }

  onState(fn) {
    this.stateHandlers.add(fn);
    return () => this.stateHandlers.delete(fn);
  }

  emitState(s) {
    this.stateHandlers.forEach((fn) => { try { fn(s); } catch { /* ignore */ } });
  }

  get connected() {
    return this.socket?.readyState === WebSocket.OPEN;
  }

  joinVideo(videoId) {
    this.send({ type: 'join', video_id: Number(videoId) });
  }
  leaveVideo(videoId) {
    this.send({ type: 'leave', video_id: Number(videoId) });
  }
  danmaku(payload) {
    return this.send({ type: 'danmaku', ...payload });
  }
  chat(to, content, clientId, videoId) {
    return this.send({ type: 'chat', to: Number(to), content, client_id: clientId, video_id: videoId || null });
  }
  typing(to) {
    this.send({ type: 'typing', to: Number(to) });
  }

  close() {
    this.closedByUser = true;
    clearInterval(this.pingTimer);
    clearTimeout(this.retryTimer);
    try { this.socket?.close(); } catch { /* ignore */ }
  }
}

export const ws = new WSClient();

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible' && !ws.connected) {
    ws.connect();
  }
});

// 登录态变化时重连，让服务端重新绑定个人频道
store.on('user', () => {
  if (ws.socket) {
    ws.closedByUser = true;
    try { ws.socket.close(); } catch { /* ignore */ }
  }
  ws.closedByUser = false;
  ws.attempt = 0;
  ws.connect();
});

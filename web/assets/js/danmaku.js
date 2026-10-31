// Canvas 弹幕引擎：分轨避让、滚动/顶部/底部、透明度/字号/速度/区域可调
const FONT_STACK = '-apple-system, "PingFang SC", "Microsoft YaHei", "Noto Sans SC", sans-serif';

export class DanmakuEngine {
  constructor(options = {}) {
    this.config = {
      enabled: true,
      opacity: 1,
      scale: 1,
      speed: 1,
      area: 1,
      bold: true,
      ...options,
    };
    this.items = [];
    this.active = [];
    this.pointer = 0;
    this.canvas = null;
    this.ctx = null;
    this.video = null;
    this.lanes = 18;
    this.width = 0;
    this.height = 0;
    this.lastTime = 0;
    this.raf = 0;
    this.dirty = true;
    this.spawnCount = 0;
  }

  attach(canvas, video) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d', { alpha: true });
    this.video = video;
    this.resize();
    this.lastTime = video.currentTime || 0;
    const loop = () => {
      if (!this.canvas) return;
      this._tick();
      this.raf = requestAnimationFrame(loop);
    };
    cancelAnimationFrame(this.raf);
    this.raf = requestAnimationFrame(loop);
  }

  resize() {
    if (!this.canvas) return;
    const rect = this.canvas.getBoundingClientRect();
    const dpr = Math.min(2.5, window.devicePixelRatio || 1);
    this.width = rect.width;
    this.height = rect.height;
    this.canvas.width = Math.max(1, Math.round(rect.width * dpr));
    this.canvas.height = Math.max(1, Math.round(rect.height * dpr));
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.lineHeight = Math.max(22, Math.round(26 * this.config.scale * Math.min(1.4, this.height / 520 + 0.4)));
    this.lanes = Math.max(3, Math.floor((this.height * this.config.area) / this.lineHeight));
    this.dirty = true;
  }

  // ==================== 数据 ====================

  setItems(list) {
    this.items = (list || [])
      .map((d) => ({
        id: d.id,
        text: String(d.content ?? ''),
        time: (Number(d.time_ms) || 0) / 1000,
        color: d.color || '#ffffff',
        mode: d.mode === 'top' || d.mode === 'bottom' ? d.mode : 'scroll',
        size: this._fontSize(d.font_size),
        username: d.username || '',
      }))
      .filter((d) => d.text)
      .sort((a, b) => a.time - b.time);
    this.active = [];
    this.pointer = this._indexAt(this.video?.currentTime || 0);
    this.dirty = true;
  }

  addLocal(item) {
    const d = {
      id: item.id || `local-${Math.random().toString(36).slice(2)}`,
      text: String(item.content ?? ''),
      time: Number(item.time_ms || 0) / 1000,
      color: item.color || '#ffffff',
      mode: item.mode === 'top' || item.mode === 'bottom' ? item.mode : 'scroll',
      size: this._fontSize(item.font_size),
      username: item.username || '我',
      local: true,
    };
    if (!d.text) return;
    let i = this.items.length;
    while (i > 0 && this.items[i - 1].time > d.time) i -= 1;
    this.items.splice(i, 0, d);
    // 插入点在游标处或之前时，游标必须后移一位；
    // 否则下一帧 tick 会把同一条再投放一次，屏幕上出现两条一模一样的弹幕
    if (i <= this.pointer) this.pointer += 1;
    if (Math.abs(d.time - (this.video?.currentTime || 0)) < 0.6) {
      // 立刻上屏，并标记已投放：否则下一次 _tick 会把同一个条目再投放一次
      d.spawned = true;
      this._spawn(d);
      this.dirty = true;
    }
  }

  clear() {
    this.active = [];
    this.dirty = true;
  }

  onSeek(t) {
    this.active = [];
    // 清掉「本地上屏」标记，保证拖回去还能再看到
    for (const it of this.items) it.spawned = false;
    this.pointer = this._indexAt(Math.max(0, t - 0.05));
    this.lastTime = t;
    this.dirty = true;
  }

  _indexAt(t) {
    let lo = 0;
    let hi = this.items.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.items[mid].time < t) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  _fontSize(serverSize) {
    const base = Number(serverSize) || 25;
    return Math.max(12, Math.min(48, base * this.config.scale * (this.width > 900 ? 1 : 0.82)));
  }

  // ==================== 配置 ====================

  setEnabled(on) {
    this.config.enabled = !!on;
    if (!on) this.clear();
    this.dirty = true;
  }
  setOpacity(v) { this.config.opacity = Math.max(0.05, Math.min(1, v)); this.dirty = true; }
  setScale(v) {
    this.config.scale = Math.max(0.5, Math.min(2, v));
    this.lineHeight = Math.max(20, Math.round(26 * this.config.scale));
    this.lanes = Math.max(3, Math.floor((this.height * this.config.area) / this.lineHeight));
    this.dirty = true;
  }
  setSpeed(v) { this.config.speed = Math.max(0.4, Math.min(2.5, v)); this.dirty = true; }
  setArea(v) {
    this.config.area = Math.max(0.25, Math.min(1, v));
    this.lanes = Math.max(3, Math.floor((this.height * this.config.area) / this.lineHeight));
    this.dirty = true;
  }

  // ==================== 渲染 ====================

  _tick() {
    if (!this.ctx || !this.canvas) return;
    const video = this.video;
    const t = video ? video.currentTime : 0;

    if (Math.abs(t - this.lastTime) > 1.2) {
      this.onSeek(t);
    }
    const dt = Math.max(0, Math.min(0.5, t - this.lastTime));
    this.lastTime = t;

    if (!this.config.enabled) {
      if (this.active.length) this.clear();
      if (this.ctx.__hadContent) {
        this.ctx.clearRect(0, 0, this.width, this.height);
        this.ctx.__hadContent = false;
      }
      return;
    }

    // 投放新弹幕
    let guard = 0;
    while (this.pointer < this.items.length && this.items[this.pointer].time <= t && guard < 40) {
      const d = this.items[this.pointer];
      if (!d.spawned && t - d.time < 1.5) this._spawn(d);
      this.pointer += 1;
      guard += 1;
    }
    if (this.pointer < this.items.length && this.items[this.pointer].time <= t) {
      while (this.pointer < this.items.length && this.items[this.pointer].time <= t) this.pointer += 1;
    }

    const ctx = this.ctx;
    ctx.clearRect(0, 0, this.width, this.height);
    ctx.globalAlpha = this.config.opacity;
    ctx.textBaseline = 'middle';

    let count = 0;
    for (let i = this.active.length - 1; i >= 0; i -= 1) {
      const it = this.active[i];
      const elapsed = t - it.start;
      let x;
      if (it.mode === 'scroll') {
        x = this.width - elapsed * it.speed;
        if (x + it.w < 0) { this.active.splice(i, 1); continue; }
      } else {
        if (elapsed > 4.2) { this.active.splice(i, 1); continue; }
        x = (this.width - it.w) / 2;
      }
      it.x = x;
      if (count++ > 220) continue;

      const y = it.y;
      const fs = it.size;
      ctx.font = `${this.config.bold ? 700 : 500} ${fs}px ${FONT_STACK}`;
      // 描边提升任何底色下的可读性
      ctx.lineWidth = Math.max(2, fs / 9);
      ctx.strokeStyle = 'rgba(0,0,0,.62)';
      ctx.lineJoin = 'round';
      ctx.strokeText(it.text, x, y);
      ctx.fillStyle = it.color;
      ctx.fillText(it.text, x, y);
    }
    ctx.globalAlpha = 1;
    ctx.__hadContent = true;

    // 暂停时不推进，只是停止生成（视觉上冻结）
    if (video && video.paused) {
      this.lastTime = t;
      this._frozenDt = dt;
    }
  }

  _spawn(d) {
    this.spawnCount += 1;
    const ctx = this.ctx;
    const t = this.video ? this.video.currentTime : 0;
    const fs = this._fontSize(d.font_size);
    ctx.font = `${this.config.bold ? 700 : 500} ${fs}px ${FONT_STACK}`;
    const w = ctx.measureText(d.text).width;
    const lh = Math.max(fs + 8, this.lineHeight);
    const maxLanes = Math.max(3, Math.floor((this.height * this.config.area) / lh));

    if (d.mode === 'scroll') {
      const speed = Math.max(60, (this.width * 1.05) / 8.2) * this.config.speed;
      let bestLane = -1;
      for (let i = 0; i < maxLanes; i += 1) {
        const occ = this._laneBusyScroll(i, t, speed, w);
        if (!occ) { bestLane = i; break; }
      }
      if (bestLane < 0) return; // 满屏丢弃，宁缺毋滥
      this.active.push({
        ...d, lane: bestLane, start: t, speed, w,
        y: lh * bestLane + lh / 2 + 6,
      });
      this._markLane(bestLane, 'scroll');
    } else {
      let lane = -1;
      for (let i = 0; i < maxLanes; i += 1) {
        if (!this._laneBusyStatic(i, t)) { lane = i; break; }
      }
      if (lane < 0) return;
      const y = d.mode === 'top'
        ? lh * lane + lh / 2 + 6
        : this.height - (lh * lane + lh / 2) - 8;
      this.active.push({ ...d, lane, start: t, speed: 0, w, y });
      this._markLane(lane, d.mode);
    }
    this.dirty = true;
  }

  _laneBusyScroll(lane, t, speed, w) {
    // 找到该轨最后一个滚动弹幕，检查其尾部是否已完全进入屏幕
    for (let i = this.active.length - 1; i >= 0; i -= 1) {
      const it = this.active[i];
      if (it.lane !== lane || it.mode !== 'scroll') continue;
      const tail = this.width - (t - it.start) * it.speed + it.w;
      return tail > this.width - 26;
    }
    return false;
  }

  _laneBusyStatic(lane, t) {
    for (let i = this.active.length - 1; i >= 0; i -= 1) {
      const it = this.active[i];
      if (it.lane === lane && it.mode !== 'scroll' && t - it.start < 4.2) return true;
    }
    return false;
  }

  _markLane() { /* 预留：可用于更精细的轨道调度 */ }

  destroy() {
    cancelAnimationFrame(this.raf);
    this.canvas = null;
    this.ctx = null;
  }
}

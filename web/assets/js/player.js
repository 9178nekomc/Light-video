// 手写播放器：自绘控制条 + 内置弹幕层 + 多分辨率切换
import { icon, esc, fmtDuration } from './ui.js';

const STORE_KEYS = {
  volume: 'lv:volume',
  rate: 'lv:rate',
  quality: 'lv:quality',
  dmOn: 'lv:dm:on',
  dmOpacity: 'lv:dm:opacity',
  dmScale: 'lv:dm:scale',
  dmSpeed: 'lv:dm:speed',
  dmArea: 'lv:dm:area',
};

function ls(key, fallback) {
  const v = localStorage.getItem(key);
  return v === null ? fallback : v;
}

export class Player extends EventTarget {
  constructor(container, opts = {}) {
    super();
    this.root = container;
    this.opts = opts;
    this.renditions = opts.renditions || [];
    this.original = { name: '原画', url: opts.src, height: opts.height || 0 };
    this.qualities = [this.original, ...this.renditions].filter((q) => q && q.url);
    this.currentQuality = 'auto';
    this.hideTimer = null;
    this.dragging = false;
    this.destroyed = false;
    this.danmaku = opts.danmaku || null;
    this._build();
    this._bind();
    this._restorePrefs();
    this.setQuality(ls(STORE_KEYS.quality, 'auto'), { silent: true, keepTime: false });
  }

  // ==================== DOM ====================

  _build() {
    const poster = this.opts.poster ? `style="background-image:url('${esc(this.opts.poster)}')"` : '';
    this.root.innerHTML = `
      <div class="player loading has-dmbar" tabindex="0" role="region" aria-label="视频播放器">
        <video playsinline webkit-playsinline preload="metadata" x5-video-player-type="h5"></video>
        <canvas class="dm-canvas"></canvas>
        <div class="player-poster" ${poster}></div>
        <button class="big-play" aria-label="播放">${icon('play')}</button>
        <div class="spinner-overlay"><div class="spinner"></div></div>
        <div class="pm-error"><div>
          <strong>播放失败</strong>
          <p class="pm-error-msg">视频加载失败，请稍后重试</p>
          <button class="btn btn-sm" data-retry>重新加载</button>
        </div></div>

        <div class="dm-bar">
          <div class="dm-color-wrap">
            <button class="dm-color" data-dm-color aria-label="弹幕颜色"></button>
            <div class="dm-color-pop" hidden></div>
          </div>
          <input class="dm-input" type="text" maxlength="80" placeholder="发个弹幕，和大家一起看…" aria-label="弹幕内容">
          <button class="dm-mode" data-dm-mode title="弹幕模式">滚动</button>
          <button class="dm-send" data-dm-send>发送</button>
        </div>

        <div class="player-controls">
          <div class="progress" role="slider" aria-label="播放进度" tabindex="0">
            <div class="progress-track">
              <div class="progress-buffer"></div>
              <div class="progress-played"></div>
              <div class="progress-thumb"></div>
            </div>
            <div class="progress-tip">0:00</div>
          </div>
          <div class="ctl-row">
            <button class="ctl play" data-act="play" aria-label="播放/暂停">${icon('play')}</button>
            <div class="vol-wrap">
              <button class="ctl" data-act="mute" aria-label="静音">${icon('volume')}</button>
              <div class="vol-slider"><input type="range" min="0" max="1" step="0.02" value="1" aria-label="音量"></div>
            </div>
            <span class="ctl-time"><span data-cur>0:00</span> / <span data-dur>0:00</span></span>
            <span class="ctl-spacer"></span>
            <button class="ctl" data-act="dm" aria-label="弹幕开关" title="弹幕开关 (D)">${icon('danmaku')}</button>
            <button class="ctl" data-act="settings" aria-label="播放设置" title="播放设置">${icon('settings')}</button>
            <button class="ctl" data-act="rate" aria-label="倍速" title="播放速度"><span class="ctl-badge" data-rate-label>1.0x</span></button>
            <button class="ctl" data-act="pip" aria-label="画中画" title="画中画">${icon('pip')}</button>
            <button class="ctl" data-act="fullscreen" aria-label="全屏" title="全屏 (F)">${icon('fullscreen')}</button>
          </div>
        </div>

        <div class="pm-panel" hidden></div>
      </div>`;

    this.el = this.root.querySelector('.player');
    // 供自动化/调试读取内部状态，不暴露到 window
    if (this.root && this.root.nodeType === 1) this.root._player = this;
    this.video = this.el.querySelector('video');
    this.canvas = this.el.querySelector('.dm-canvas');
    this.progress = this.el.querySelector('.progress');
    this.played = this.el.querySelector('.progress-played');
    this.buffer = this.el.querySelector('.progress-buffer');
    this.thumb = this.el.querySelector('.progress-thumb');
    this.tip = this.el.querySelector('.progress-tip');
    this.curEl = this.el.querySelector('[data-cur]');
    this.durEl = this.el.querySelector('[data-dur]');
    this.panel = this.el.querySelector('.pm-panel');

    this.video.src = this.opts.src || '';
    if (this.opts.poster) this.video.poster = this.opts.poster;
    this.video.volume = Number(ls(STORE_KEYS.volume, 1));
    this.video.playbackRate = Number(ls(STORE_KEYS.rate, 1));

    if (this.danmaku) this.danmaku.attach(this.canvas, this.video);
  }

  // ==================== 事件 ====================

  _bind() {
    const v = this.video;
    const el = this.el;

    el.addEventListener('mousemove', () => this._showUi());
    el.addEventListener('mouseleave', () => { if (!v.paused) this._hideUi(true); });
    el.addEventListener('click', (e) => {
      if (e.target.closest('.player-controls, .dm-bar, .pm-panel, .big-play')) return;
      if (matchMedia('(pointer: coarse)').matches) {
        el.classList.contains('hide-ui') ? this._showUi() : this._hideUi(true);
        return;
      }
      this.toggle();
    });
    el.addEventListener('dblclick', (e) => {
      if (e.target.closest('.player-controls, .dm-bar, .pm-panel')) return;
      this.toggleFullscreen();
    });

    // 指针停在控制条 / 弹幕栏 / 设置面板上时不自动隐藏
    this.uiPinned = false;
    for (const sel of ['.player-controls', '.dm-bar', '.pm-panel']) {
      const node = el.querySelector(sel);
      if (!node) continue;
      node.addEventListener('pointerenter', () => {
        this.uiPinned = true;
        this._showUi(true);
      });
      node.addEventListener('pointerleave', () => {
        this.uiPinned = false;
        this._hideUi();
      });
    }

    el.querySelector('.big-play').addEventListener('click', () => this.play());
    el.querySelector('[data-retry]').addEventListener('click', () => {
      el.querySelector('.pm-error').classList.remove('show');
      el.classList.remove('has-error');
      this.video.load();
      this.play();
    });

    v.addEventListener('play', () => {
      el.classList.add('playing');
      el.classList.remove('loading');
      this._updatePlayIcon(true);
      this._hideUi(true);
      this.dispatchEvent(new Event('play'));
    });
    v.addEventListener('pause', () => {
      el.classList.remove('playing');
      this._updatePlayIcon(false);
      this._showUi(true);
      this.dispatchEvent(new Event('pause'));
    });
    v.addEventListener('waiting', () => el.classList.add('buffering'));
    v.addEventListener('playing', () => el.classList.remove('buffering'));
    v.addEventListener('canplay', () => {
      el.classList.remove('loading', 'buffering', 'has-error');
      el.querySelector('.pm-error').classList.remove('show');
    });
    v.addEventListener('error', () => {
      el.classList.remove('loading', 'buffering');
      el.classList.add('has-error');
      el.querySelector('.pm-error').classList.add('show');
      const msg = v.error?.code === 4 ? '该格式暂不支持直接播放，请尝试切换画质或使用其他浏览器' : '视频加载失败，请稍后重试';
      el.querySelector('.pm-error-msg').textContent = msg;
    });
    v.addEventListener('timeupdate', () => this._syncTime());
    v.addEventListener('progress', () => this._syncBuffer());
    v.addEventListener('durationchange', () => {
      this.durEl.textContent = fmtDuration((v.duration || 0) * 1000);
      this.dispatchEvent(new CustomEvent('duration', { detail: (v.duration || 0) * 1000 }));
    });
    v.addEventListener('volumechange', () => this._syncVolume());
    v.addEventListener('ended', () => {
      el.classList.remove('playing');
      this.dispatchEvent(new Event('ended'));
    });

    el.querySelector('[data-act=play]').addEventListener('click', () => this.toggle());
    el.querySelector('[data-act=mute]').addEventListener('click', () => {
      v.muted = !v.muted;
      if (!v.muted && v.volume === 0) v.volume = 0.8;
    });
    el.querySelector('.vol-slider input').addEventListener('input', (e) => {
      v.volume = Number(e.target.value);
      v.muted = v.volume === 0;
      localStorage.setItem(STORE_KEYS.volume, String(v.volume));
    });
    el.querySelector('[data-act=dm]').addEventListener('click', () => this.toggleDanmaku());
    el.querySelector('[data-act=fullscreen]').addEventListener('click', () => this.toggleFullscreen());
    el.querySelector('[data-act=pip]').addEventListener('click', () => this.togglePip());
    el.querySelector('[data-act=settings]').addEventListener('click', () => this._showSettings());
    el.querySelector('[data-act=rate]').addEventListener('click', () => this._showRatePanel());

    this._bindProgress();
    this._bindDanmakuBar();
    document.addEventListener('fullscreenchange', () => {
      const on = document.fullscreenElement === el;
      el.querySelector('[data-act=fullscreen]').innerHTML = icon(on ? 'exitFullscreen' : 'fullscreen');
      if (!on) setTimeout(() => this.danmaku?.resize(), 60);
    });
    this._onKey = (e) => this._handleKey(e);
    document.addEventListener('keydown', this._onKey);

    if (window.ResizeObserver) {
      this._ro = new ResizeObserver(() => this.danmaku?.resize());
      this._ro.observe(el);
    }
  }

  _bindProgress() {
    const p = this.progress;
    const ratio = (clientX) => {
      const r = p.getBoundingClientRect();
      return Math.min(1, Math.max(0, (clientX - r.left) / r.width));
    };
    const preview = (clientX) => {
      const r = p.getBoundingClientRect();
      const ratio2 = Math.min(1, Math.max(0, (clientX - r.left) / r.width));
      const dur = this.video.duration || 0;
      this.tip.style.left = `${ratio2 * 100}%`;
      this.tip.textContent = fmtDuration(ratio2 * dur * 1000);
      return ratio2;
    };

    p.addEventListener('pointermove', (e) => { if (!this.dragging) preview(e.clientX); });
    p.addEventListener('pointerdown', (e) => {
      this.dragging = true;
      p.classList.add('dragging');
      p.setPointerCapture(e.pointerId);
      const r = preview(e.clientX);
      this._paintProgress(r);
    });
    p.addEventListener('pointerup', (e) => {
      if (!this.dragging) return;
      this.dragging = false;
      p.classList.remove('dragging');
      const r = ratio(e.clientX);
      this.seek(r * (this.video.duration || 0));
      this.dispatchEvent(new CustomEvent('seek', { detail: r }));
    });
    p.addEventListener('pointercancel', () => {
      this.dragging = false;
      p.classList.remove('dragging');
      this._syncTime();
    });
  }

  _bindDanmakuBar() {
    const input = this.el.querySelector('.dm-input');
    const send = this.el.querySelector('[data-dm-send]');
    const colorBtn = this.el.querySelector('[data-dm-color]');
    const colorPop = this.el.querySelector('.dm-color-pop');
    const modeBtn = this.el.querySelector('[data-dm-mode]');
    this.dmColor = ls('lv:dm:color', '#ffffff');
    this.dmMode = 'scroll';

    const COLORS = ['#ffffff', '#ff5c8a', '#ffd166', '#7ee8d8', '#7aa2ff', '#c084fc', '#ff9f6e', '#98f5a0', '#ff6b6b', '#a0f0ff', '#f7a8ff', '#d4d8e2'];
    colorPop.innerHTML = COLORS.map((c) => `<button data-c="${c}" style="background:${c}" aria-label="颜色 ${c}"></button>`).join('');
    const paintColor = () => {
      colorBtn.style.background = this.dmColor;
      colorPop.querySelectorAll('[data-c]').forEach((b) => b.classList.toggle('on', b.dataset.c === this.dmColor));
    };
    paintColor();
    colorBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      colorPop.hidden = !colorPop.hidden;
    });
    colorPop.addEventListener('click', (e) => {
      const b = e.target.closest('[data-c]');
      if (!b) return;
      this.dmColor = b.dataset.c;
      localStorage.setItem('lv:dm:color', this.dmColor);
      paintColor();
      colorPop.hidden = true;
    });
    document.addEventListener('click', (e) => {
      if (!e.target.closest('.dm-color-wrap')) colorPop.hidden = true;
    });

    const MODES = [['scroll', '滚动'], ['top', '顶部'], ['bottom', '底部']];
    modeBtn.addEventListener('click', () => {
      const i = MODES.findIndex((m) => m[0] === this.dmMode);
      this.dmMode = MODES[(i + 1) % MODES.length][0];
      modeBtn.textContent = MODES.find((m) => m[0] === this.dmMode)[1];
    });

    const submit = () => {
      const content = input.value.trim();
      if (!content) return;
      const detail = {
        content,
        time_ms: Math.round((this.video.currentTime || 0) * 1000),
        color: this.dmColor,
        mode: this.dmMode,
        font_size: 25,
      };
      // dispatchEvent 返回 false 表示页面已 preventDefault 接管发送，
      // 此时不要本地回显，否则会和服务端广播各加一条
      const canceled = !this.dispatchEvent(new CustomEvent('danmaku', { detail, cancelable: true }));
      input.value = '';
      if (!canceled) {
        this.danmaku?.addLocal({ ...detail, id: `local-${Date.now()}`, username: '我' });
      }
      this._showUi(true);
    };
    send.addEventListener('click', submit);
    input.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Enter') { e.preventDefault(); submit(); }
    });
    input.addEventListener('focus', () => {
      this.uiPinned = true;
      this._showUi(true);
    });
    input.addEventListener('blur', () => {
      this.uiPinned = false;
      this._hideUi();
    });
  }

  _handleKey(e) {
    const tag = document.activeElement?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || document.activeElement?.isContentEditable) return;
    if (this.el.closest('[hidden]')) return;
    const v = this.video;
    switch (e.key) {
      case ' ':
      case 'k':
        e.preventDefault(); this.toggle(); break;
      case 'ArrowLeft':
        e.preventDefault(); this.seek(v.currentTime - 5); break;
      case 'ArrowRight':
        e.preventDefault(); this.seek(v.currentTime + 5); break;
      case 'ArrowUp':
        e.preventDefault(); v.volume = Math.min(1, v.volume + 0.08); break;
      case 'ArrowDown':
        e.preventDefault(); v.volume = Math.max(0, v.volume - 0.08); break;
      case 'm': v.muted = !v.muted; break;
      case 'f': this.toggleFullscreen(); break;
      case 'd': this.toggleDanmaku(); break;
      case 'p': this.togglePip(); break;
      case ']': this.setRate(Math.min(2, v.playbackRate + 0.25)); break;
      case '[': this.setRate(Math.max(0.5, v.playbackRate - 0.25)); break;
      default: break;
    }
  }

  // ==================== 播放控制 ====================

  play() {
    return this.video.play().catch(() => {});
  }
  pause() { this.video.pause(); }
  toggle() {
    if (this.video.paused) this.play(); else this.pause();
  }
  seek(seconds) {
    if (!Number.isFinite(seconds)) return;
    const dur = this.video.duration || 0;
    this.video.currentTime = Math.max(0, Math.min(dur ? dur - 0.05 : seconds, seconds));
    this._syncTime();
    this.danmaku?.onSeek(this.video.currentTime);
  }
  setRate(rate) {
    this.video.playbackRate = rate;
    localStorage.setItem(STORE_KEYS.rate, String(rate));
    this.el.querySelector('[data-rate-label]').textContent = `${rate.toFixed(2).replace(/0$/, '')}x`;
    document.querySelector(`.pm-panel [data-rate="${rate}"]`)?.parentElement?.querySelectorAll('button').forEach((b) => b.classList.toggle('on', Number(b.dataset.rate) === rate));
    this.dispatchEvent(new CustomEvent('rate', { detail: rate }));
  }
  setQuality(name, { silent = false, keepTime = true } = {}) {
    const q = name === 'auto' ? this._autoQuality() : this.qualities.find((x) => x.name === name);
    if (!q) return;
    this.currentQuality = name;
    localStorage.setItem(STORE_KEYS.quality, name);
    const t = this.video.currentTime;
    const wasPlaying = !this.video.paused;
    const target = q.url;

    if (this.video.currentSrc && this.video.currentSrc.endsWith(target)) return;

    this.el.classList.add('loading', 'buffering');
    this.video.src = target;
    this.video.load();
    if (keepTime && t > 0.2) {
      this.video.addEventListener('loadedmetadata', () => {
        try { this.video.currentTime = t; } catch { /* ignore */ }
        if (wasPlaying) this.play();
        this.danmaku?.onSeek(t);
      }, { once: true });
    } else if (wasPlaying) {
      this.play();
    }
    if (!silent) this.dispatchEvent(new CustomEvent('quality', { detail: { name, quality: q } }));
  }
  _autoQuality() {
    const h = this.el.clientHeight || 480;
    const targetH = Math.round(h * Math.min(2, window.devicePixelRatio || 1));
    const sorted = [...this.renditions].sort((a, b) => a.height - b.height);
    const pick = sorted.find((r) => r.height >= targetH) || sorted[sorted.length - 1];
    return pick || this.original;
  }
  toggleFullscreen() {
    if (document.fullscreenElement) {
      document.exitFullscreen?.();
    } else {
      this.el.requestFullscreen?.().catch(() => {
        this.video.webkitEnterFullscreen?.();
      });
      this._showUi(true);
    }
  }
  async togglePip() {
    try {
      if (document.pictureInPictureElement) await document.exitPictureInPicture();
      else await this.video.requestPictureInPicture();
    } catch {
      /* 浏览器不支持 */
    }
  }
  toggleDanmaku() {
    if (!this.danmaku) return;
    const on = !this.danmaku.config.enabled;
    this.danmaku.setEnabled(on);
    localStorage.setItem(STORE_KEYS.dmOn, on ? '1' : '0');
    this.el.querySelector('[data-act=dm]').style.opacity = on ? '1' : '.45';
    this.dispatchEvent(new CustomEvent('danmaku-toggle', { detail: on }));
  }
  setTheater(on) {
    this.el.classList.toggle('theater', on);
    setTimeout(() => this.danmaku?.resize(), 80);
  }

  // ==================== UI 同步 ====================

  _restorePrefs() {
    if (!this.danmaku) return;
    const on = ls(STORE_KEYS.dmOn, '1') === '1';
    this.danmaku.setEnabled(on);
    this.el.querySelector('[data-act=dm]').style.opacity = on ? '1' : '.45';
    this.danmaku.setOpacity(Number(ls(STORE_KEYS.dmOpacity, 1)));
    this.danmaku.setScale(Number(ls(STORE_KEYS.dmScale, 1)));
    this.danmaku.setSpeed(Number(ls(STORE_KEYS.dmSpeed, 1)));
    this.danmaku.setArea(Number(ls(STORE_KEYS.dmArea, 1)));
    this.setRate(Number(ls(STORE_KEYS.rate, 1)));
    this._syncVolume();
  }

  _updatePlayIcon(playing) {
    const btn = this.el.querySelector('[data-act=play]');
    btn.innerHTML = icon(playing ? 'pause' : 'play');
    btn.setAttribute('aria-label', playing ? '暂停' : '播放');
  }

  _syncTime() {
    if (this.dragging) return;
    const v = this.video;
    const dur = v.duration || 0;
    const r = dur ? v.currentTime / dur : 0;
    this._paintProgress(r);
    this.curEl.textContent = fmtDuration(v.currentTime * 1000);
    this.durEl.textContent = fmtDuration(dur * 1000);
  }

  _paintProgress(ratio) {
    const pct = `${(ratio * 100).toFixed(3)}%`;
    this.played.style.width = pct;
    this.thumb.style.left = pct;
  }

  _syncBuffer() {
    const v = this.video;
    if (!v.buffered.length || !v.duration) return;
    let end = 0;
    for (let i = 0; i < v.buffered.length; i += 1) {
      if (v.buffered.start(i) <= v.currentTime + 0.5) end = Math.max(end, v.buffered.end(i));
    }
    this.buffer.style.width = `${Math.min(100, (end / v.duration) * 100)}%`;
  }

  _syncVolume() {
    const v = this.video;
    const pct = v.muted ? 0 : v.volume;
    this.el.querySelector('.vol-slider input').value = String(pct);
    this.el.querySelector('[data-act=mute]').innerHTML = icon(pct === 0 ? 'mute' : 'volume');
  }

  _showUi(force = false) {
    this.el.classList.remove('hide-ui');
    clearTimeout(this.hideTimer);
    if (!force) this._hideUi(false);
  }
  _hideUi(now = false) {
    clearTimeout(this.hideTimer);
    if (this.video.paused) return;
    if (this.uiPinned) return;
    if (this.panel.hidden === false) return;
    // 正在输入弹幕时绝不收起
    const active = document.activeElement;
    if (active && this.el.contains(active) && active.tagName === 'INPUT') return;
    const hide = () => {
      if (!this.video.paused && this.panel.hidden) this.el.classList.add('hide-ui');
    };
    if (now) hide();
    else this.hideTimer = setTimeout(hide, 2400);
  }

  // ==================== 设置面板 ====================

  _closePanel() {
    this.panel.hidden = true;
  }

  _showSettings() {
    if (!this.panel.hidden && this.panel.dataset.kind === 'settings') { this._closePanel(); return; }
    this.panel.dataset.kind = 'settings';
    const d = this.danmaku?.config || {};
    this.panel.innerHTML = `
      <div class="panel-title">弹幕设置</div>
      <div class="row"><span>显示弹幕</span><button class="switch ${d.enabled ? 'on' : ''}" data-s="dm"></button></div>
      <div class="row"><span>不透明度</span><input type="range" min="0.15" max="1" step="0.05" value="${d.opacity ?? 1}" data-s="opacity"></div>
      <div class="row"><span>字号</span><input type="range" min="0.6" max="1.8" step="0.05" value="${d.scale ?? 1}" data-s="scale"></div>
      <div class="row"><span>速度</span><input type="range" min="0.5" max="2" step="0.1" value="${d.speed ?? 1}" data-s="speed"></div>
      <div class="row"><span>显示区域</span><input type="range" min="0.25" max="1" step="0.25" value="${d.area ?? 1}" data-s="area"></div>
      <div class="sep"></div>
      <div class="panel-title">画面</div>
      <button data-s="quality">清晰度 <span class="tick">${this.currentQuality === 'auto' ? `自动 (${this._autoQuality().name})` : this.currentQuality} ›</span></button>
      <button data-s="theater">影院模式</button>`;
    this.panel.hidden = false;

    this.panel.querySelectorAll('input[type=range]').forEach((inp) => {
      inp.addEventListener('input', (e) => {
        const key = e.target.dataset.s;
        const val = Number(e.target.value);
        if (key === 'opacity') { this.danmaku.setOpacity(val); localStorage.setItem(STORE_KEYS.dmOpacity, String(val)); }
        if (key === 'scale') { this.danmaku.setScale(val); localStorage.setItem(STORE_KEYS.dmScale, String(val)); }
        if (key === 'speed') { this.danmaku.setSpeed(val); localStorage.setItem(STORE_KEYS.dmSpeed, String(val)); }
        if (key === 'area') { this.danmaku.setArea(val); localStorage.setItem(STORE_KEYS.dmArea, String(val)); }
      });
    });
    this.panel.querySelector('[data-s=dm]').addEventListener('click', (e) => {
      this.toggleDanmaku();
      e.currentTarget.classList.toggle('on', this.danmaku.config.enabled);
    });
    this.panel.querySelector('[data-s=quality]').addEventListener('click', () => this._showQualityPanel());
    this.panel.querySelector('[data-s=theater]').addEventListener('click', () => {
      this.setTheater(!this.el.classList.contains('theater'));
      this._closePanel();
    });
  }

  _showQualityPanel() {
    this.panel.dataset.kind = 'quality';
    const rows = [`<button data-q="auto" class="${this.currentQuality === 'auto' ? 'on' : ''}">自动 <span class="tick">${this.currentQuality === 'auto' ? '✓' : ''}</span></button>`]
      .concat(this.qualities.map((q) => `<button data-q="${esc(q.name)}" class="${this.currentQuality === q.name ? 'on' : ''}">${esc(q.name)}${q.height ? ` · ${q.height}p` : ''}<span class="tick">${this.currentQuality === q.name ? '✓' : ''}</span></button>`));
    this.panel.innerHTML = `<div class="panel-title">清晰度</div>${rows.join('')}
      ${this.opts.hlsUrl ? '<div class="sep"></div><div class="row" style="font-size:11.5px;opacity:.6">已启用多码率转码</div>' : ''}`;
    this.panel.hidden = false;
    this.panel.querySelectorAll('[data-q]').forEach((b) => {
      b.addEventListener('click', () => {
        this.setQuality(b.dataset.q);
        this._closePanel();
      });
    });
  }

  _showRatePanel() {
    if (!this.panel.hidden && this.panel.dataset.kind === 'rate') { this._closePanel(); return; }
    this.panel.dataset.kind = 'rate';
    const rates = [0.5, 0.75, 1, 1.25, 1.5, 2];
    this.panel.innerHTML = `<div class="panel-title">播放速度</div>${rates.map((r) => `
      <button data-rate="${r}" class="${this.video.playbackRate === r ? 'on' : ''}">${r === 1 ? '正常' : `${r}x`}<span class="tick">${this.video.playbackRate === r ? '✓' : ''}</span></button>`).join('')}`;
    this.panel.hidden = false;
    this.panel.querySelectorAll('[data-rate]').forEach((b) => {
      b.addEventListener('click', () => { this.setRate(Number(b.dataset.rate)); this._closePanel(); });
    });
  }

  destroy() {
    this.destroyed = true;
    clearTimeout(this.hideTimer);
    document.removeEventListener('keydown', this._onKey);
    this._ro?.disconnect();
    this.video.pause();
    this.video.removeAttribute('src');
    this.video.load();
    this.danmaku?.destroy();
    this.root.innerHTML = '';
  }
}

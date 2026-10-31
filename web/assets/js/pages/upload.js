import { api, uploadFile, uploadCover } from '../api.js';
import { grabPoster } from '../thumb.js';
import { store } from '../store.js';
import { icon, esc, fmtBytes, toast } from '../ui.js';
import { navigate } from '../router.js';

const CATS = [['anime', '动画'], ['game', '游戏'], ['music', '音乐'], ['tech', '科技'], ['life', '生活'], ['movie', '影视'], ['study', '学习'], ['other', '其他']];
const MAX_MB = 4096;

export async function renderUpload({ view }) {
  if (!store.user) {
    view.innerHTML = `
      <div class="auth-wrap"><div class="auth-card" style="text-align:center">
        <h1>需要登录</h1>
        <p class="sub">登录后即可投稿你的作品</p>
        <a class="btn btn-primary btn-block btn-lg" href="/login?next=/upload">去登录</a>
      </div></div>`;
    return;
  }

  view.innerHTML = `
    <div class="upload-wrap">
      <div class="section-head" style="margin-top:0"><h2>投稿</h2><span class="hint">支持 MP4 / WebM / MOV / MKV，最大 ${MAX_MB / 1024} GB</span></div>

      <div class="upload-drop" id="drop" tabindex="0" role="button">
        <div class="ico">${icon('upload')}</div>
        <h3>拖拽视频到这里，或点击选择文件</h3>
        <p>视频会按 4MB 分片并行上传，支持断点续传与失败重试</p>
        <input type="file" id="file" accept="video/mp4,video/webm,video/quicktime,video/x-matroska,video/x-msvideo" hidden>
      </div>

      <div id="file-info" hidden></div>

      <form id="meta-form" hidden>
        <div class="form-grid">
          <div>
            <div class="field">
              <label for="m-title">标题 <span class="hint">（2-100 字）</span></label>
              <input class="input" id="m-title" maxlength="100" placeholder="给视频起个吸引人的标题">
            </div>
            <div class="field">
              <label for="m-desc">简介</label>
              <textarea class="textarea" id="m-desc" maxlength="5000" placeholder="介绍一下这个视频的内容、亮点…"></textarea>
            </div>
            <div class="field">
              <label for="m-tags">标签 <span class="hint">（用逗号分隔，最多 8 个）</span></label>
              <input class="input" id="m-tags" maxlength="140" placeholder="例如：Rust, 编程, 教程">
            </div>
          </div>
          <div>
            <div class="field">
              <label for="m-cat">分区</label>
              <select class="select" id="m-cat">${CATS.map(([v, n]) => `<option value="${v}">${n}</option>`).join('')}</select>
            </div>
            <div class="field">
              <label for="m-vis">可见性</label>
              <select class="select" id="m-vis">
                <option value="public">公开</option>
                <option value="unlisted">不列出（仅链接可见）</option>
                <option value="private">私密（仅自己）</option>
              </select>
            </div>
            <div class="field">
              <label>封面</label>
              <div class="cover-pick">
                <div class="cover-preview" id="cover-preview">${icon('image')}<span style="margin-left:6px">未选择</span></div>
                <div>
                  <button type="button" class="btn btn-sm" id="cover-btn">${icon('image')} 选择图片</button>
                  <div class="hint" id="cover-hint" style="margin-top:6px">不选择时自动从视频截帧；JPG / PNG / WebP，≤ 12MB</div>
                  <input type="file" id="cover-file" accept="image/jpeg,image/png,image/webp,image/gif" hidden>
                </div>
              </div>
            </div>
          </div>
        </div>

        <div class="up-file" id="up-status" hidden>
          <div class="fi">${icon('video')}</div>
          <div style="flex:1;min-width:0">
            <div class="fname" id="up-name">—</div>
            <div class="fmeta" id="up-meta">等待上传</div>
            <div class="upbar"><div id="up-bar"></div></div>
          </div>
          <button type="button" class="btn btn-sm" id="up-cancel">取消</button>
        </div>

        <div style="display:flex;gap:10px;align-items:center;margin-top:8px">
          <button class="btn btn-primary btn-lg" type="submit" id="submit">${icon('upload')} 发布</button>
          <span class="hint" id="submit-hint"></span>
        </div>
      </form>
    </div>`;

  const drop = document.getElementById('drop');
  const fileInput = document.getElementById('file');
  const info = document.getElementById('file-info');
  const form = document.getElementById('meta-form');
  let file = null;
  let coverFile = null;
  let coverKey = '';
  let controller = null;

  const pick = () => fileInput.click();
  drop.addEventListener('click', pick);
  drop.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); pick(); } });
  drop.addEventListener('dragover', (e) => { e.preventDefault(); drop.classList.add('drag'); });
  drop.addEventListener('dragleave', () => drop.classList.remove('drag'));
  drop.addEventListener('drop', (e) => {
    e.preventDefault();
    drop.classList.remove('drag');
    const f = e.dataTransfer?.files?.[0];
    if (f) acceptFile(f);
  });
  fileInput.addEventListener('change', () => { if (fileInput.files[0]) acceptFile(fileInput.files[0]); });

  function acceptFile(f) {
    const ok = /\.(mp4|webm|mov|m4v|mkv|avi|flv)$/i.test(f.name) || f.type.startsWith('video/');
    if (!ok) { toast('不支持的视频格式', 'err'); return; }
    if (f.size > MAX_MB * 1024 * 1024) { toast(`文件超过 ${MAX_MB / 1024} GB`, 'err'); return; }
    if (f.size === 0) { toast('文件为空', 'err'); return; }
    file = f;
    const base = f.name.replace(/\.[^.]+$/, '');
    document.getElementById('m-title').value = base.slice(0, 100);
    drop.hidden = true;
    info.hidden = false;
    info.innerHTML = `
      <div class="up-file">
        <div class="fi">${icon('video')}</div>
        <div style="flex:1;min-width:0">
          <div class="fname">${esc(f.name)}</div>
          <div class="fmeta">${fmtBytes(f.size)} · ${f.type || 'video'} · 将分为 ${Math.ceil(f.size / (4 * 1024 * 1024))} 个分片</div>
        </div>
        <button class="btn btn-sm fx" id="reselect">重新选择</button>
      </div>`;
    info.querySelector('#reselect').addEventListener('click', () => {
      file = null;
      info.hidden = true;
      drop.hidden = false;
      form.hidden = true;
      fileInput.value = '';
    });
    form.hidden = false;
    document.getElementById('m-title').focus();
    autoPoster(f);
  }

  // 没选封面时，自动从视频截一帧（不需要 ffmpeg）
  async function autoPoster(f) {
    const preview = document.getElementById('cover-preview');
    const hint = document.getElementById('cover-hint');
    if (coverFile || coverKey) return;
    if (hint) hint.textContent = '正在从视频截取封面…';
    try {
      const blob = await grabPoster(f, { at: 1.2 });
      if (coverFile || coverKey) return; // 期间用户自己选了封面，优先用户
      const file2 = new File([blob], 'poster.jpg', { type: 'image/jpeg' });
      const r = await uploadCover(file2);
      if (coverFile || coverKey) return;
      coverKey = r.cover_key;
      preview.innerHTML = `<img src="${r.cover_url}" alt="自动封面">`;
      if (hint) hint.textContent = '已自动截取封面，可点击更换';
    } catch {
      if (hint) hint.textContent = '自动截帧失败，可手动选择图片';
    }
  }

  // 封面
  document.getElementById('cover-btn').addEventListener('click', () => document.getElementById('cover-file').click());
  document.getElementById('cover-file').addEventListener('change', async (e) => {
    const f = e.target.files[0];
    if (!f) return;
    if (f.size > 12 * 1024 * 1024) { toast('封面不能超过 12MB', 'err'); return; }
    coverFile = f;
    const preview = document.getElementById('cover-preview');
    const url = URL.createObjectURL(f);
    preview.innerHTML = `<img src="${url}" alt="封面预览">`;
    try {
      const r = await uploadCover(f);
      coverKey = r.cover_key;
      toast('封面上传成功', 'ok');
    } catch (err) {
      coverKey = '';
      toast(err.message, 'err');
    }
  });

  // 提交
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    if (!file) return;
    const title = document.getElementById('m-title').value.trim();
    if (title.length < 2) { toast('标题至少 2 个字', 'err'); return; }

    const submit = document.getElementById('submit');
    submit.disabled = true;
    submit.innerHTML = `<span class="spinner"></span> 上传中`;

    const status = document.getElementById('up-status');
    status.hidden = false;
    document.getElementById('up-name').textContent = file.name;

    controller = new AbortController();
    try {
      const tags = document.getElementById('m-tags').value.split(/[,，\s]+/).map((s) => s.trim()).filter(Boolean).slice(0, 8);
      const result = await uploadFile(file, {
        signal: controller.signal,
        meta: {
          title,
          description: document.getElementById('m-desc').value.trim(),
          category: document.getElementById('m-cat').value,
          visibility: document.getElementById('m-vis').value,
          tags,
          cover_key: coverKey,
        },
        onProgress: (p) => {
          // 页面可能在完成后已跳转，所有节点访问都要判空
          const bar = document.getElementById('up-bar');
          const meta = document.getElementById('up-meta');
          const hint = document.getElementById('submit-hint');
          if (bar) bar.style.width = `${p.percent}%`;
          if (meta) {
            meta.textContent = p.assembling
              ? '分片合并与校验中…'
              : `${fmtBytes(p.uploaded)} / ${fmtBytes(p.total)} · ${Math.round(p.percent)}%`;
          }
          if (hint) hint.textContent = p.assembling ? '正在组装文件…' : '';
        },
      });
      toast(result.status === 'processing' ? '上传完成，正在转码' : '发布成功', 'ok');
      navigate(`/watch/${result.video_id}`);
    } catch (err) {
      if (err.name !== 'AbortError') {
        toast(err.message || '上传失败', 'err');
        const meta = document.getElementById('up-meta');
        if (meta) meta.textContent = '上传失败，可重试';
      }
    } finally {
      submit.disabled = false;
      submit.innerHTML = `${icon('upload')} 发布`;
      const hint = document.getElementById('submit-hint');
      if (hint) hint.textContent = '';
    }
  });

  document.getElementById('up-cancel').addEventListener('click', () => {
    controller?.abort();
    document.getElementById('up-status').hidden = true;
    toast('已取消上传', 'info');
  });

  return () => controller?.abort();
}

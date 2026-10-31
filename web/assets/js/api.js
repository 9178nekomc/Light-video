// 统一 API 客户端：CSRF、错误归一化、分片上传
const AUTH_LOST = new Set();
export function onAuthLost(fn) { AUTH_LOST.add(fn); }

export class ApiError extends Error {
  constructor(message, { status = 0, code = 'error', data = null } = {}) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.data = data;
  }
}

function cookie(name) {
  const m = document.cookie.match(new RegExp('(?:^|; )' + name + '=([^;]*)'));
  return m ? decodeURIComponent(m[1]) : '';
}

function headers(extra) {
  const h = { Accept: 'application/json', ...extra };
  const csrf = cookie('lv_csrf');
  if (csrf) h['X-CSRF-Token'] = csrf;
  return h;
}

async function parse(res) {
  const text = await res.text();
  if (!text) return null;
  try { return JSON.parse(text); } catch { return { raw: text }; }
}

export async function api(path, { method = 'GET', body, signal, silent = false } = {}) {
  const opts = { method, credentials: 'same-origin', headers: headers(), signal };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  let res;
  try {
    res = await fetch(path, opts);
  } catch (e) {
    if (e.name === 'AbortError') throw e;
    throw new ApiError('网络连接失败，请检查网络', { code: 'network' });
  }

  const data = await parse(res);
  if (!res.ok) {
    const err = data?.error || {};
    if (res.status === 401 && !silent) AUTH_LOST.forEach((f) => f());
    throw new ApiError(err.message || `请求失败 (${res.status})`, {
      status: res.status,
      code: err.code || 'http_error',
      data,
    });
  }
  return data && typeof data === 'object' && 'data' in data ? data.data : data;
}

export async function apiRaw(path, opts) {
  const res = await fetch(path, { credentials: 'same-origin', headers: headers(), ...opts });
  return res;
}

// ==================== 分片上传 ====================

async function putChunk(uploadId, index, blob, onProgress, signal) {
  // XHR 才能拿到上传进度（fetch 无上传进度事件）
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('PUT', `/api/uploads/${uploadId}/chunk/${index}`, true);
    xhr.withCredentials = true;
    const csrf = cookie('lv_csrf');
    if (csrf) xhr.setRequestHeader('X-CSRF-Token', csrf);
    xhr.setRequestHeader('Content-Type', 'application/octet-stream');
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable) onProgress(index, e.loaded, e.total);
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) return resolve();
      let msg = xhr.status === 413
        ? `分片 ${index} 超过服务端允许的大小（413），请检查 CHUNK_SIZE_MB 配置`
        : `分片 ${index} 上传失败 (${xhr.status})`;
      try {
        const j = JSON.parse(xhr.responseText);
        if (j?.error?.message) msg = j.error.message;
      } catch { /* ignore */ }
      reject(new ApiError(msg, { status: xhr.status }));
    };
    xhr.onerror = () => reject(new ApiError('分片上传网络错误'));
    xhr.onabort = () => reject(new DOMException('aborted', 'AbortError'));
    if (signal) signal.addEventListener('abort', () => xhr.abort(), { once: true });
    xhr.send(blob);
  });
}

/**
 * 分片上传：并发 3、失败重试 3 次、整体字节级进度
 */
export async function uploadFile(file, { meta, onProgress, signal, concurrency = 3 } = {}) {
  const init = await api('/api/uploads/init', {
    method: 'POST',
    body: { filename: file.name, total_size: file.size },
    signal,
  });

  const { upload_id, chunk_size, total_chunks } = init;
  const loaded = new Array(total_chunks).fill(0);
  const report = () => {
    const done = loaded.reduce((a, b) => a + b, 0);
    onProgress?.({
      percent: Math.min(99, (done / file.size) * 100),
      uploaded: done,
      total: file.size,
      chunks: total_chunks,
    });
  };

  let cursor = 0;
  let failed = null;

  const worker = async () => {
    while (cursor < total_chunks && !failed) {
      const index = cursor++;
      const start = index * chunk_size;
      const blob = file.slice(start, Math.min(start + chunk_size, file.size));
      let attempt = 0;
      for (;;) {
        try {
          await putChunk(upload_id, index, blob, (i, loadedBytes) => {
            loaded[i] = loadedBytes;
            report();
          }, signal);
          loaded[index] = blob.size;
          report();
          break;
        } catch (e) {
          if (e.name === 'AbortError') throw e;
          attempt += 1;
          if (attempt >= 3) {
            failed = e;
            throw e;
          }
          await new Promise((r) => setTimeout(r, 400 * attempt));
        }
      }
    }
  };

  try {
    await Promise.all(Array.from({ length: Math.min(concurrency, total_chunks) }, worker));
  } catch (e) {
    api(`/api/uploads/${upload_id}`, { method: 'DELETE', silent: true }).catch(() => {});
    throw e;
  }

  onProgress?.({ percent: 99, uploaded: file.size, total: file.size, chunks: total_chunks, assembling: true });

  const result = await api(`/api/uploads/${upload_id}/complete`, {
    method: 'POST',
    signal,
    body: {
      title: meta.title,
      description: meta.description || '',
      category: meta.category || 'other',
      tags: meta.tags || [],
      cover_key: meta.cover_key || '',
      visibility: meta.visibility || 'public',
    },
  });
  onProgress?.({ percent: 100, uploaded: file.size, total: file.size, chunks: total_chunks, done: true });
  return result;
}

export async function uploadCover(file, { signal } = {}) {
  const fd = new FormData();
  fd.append('cover', file, file.name);
  const res = await fetch('/api/uploads/cover', {
    method: 'POST',
    credentials: 'same-origin',
    headers: headers(),
    body: fd,
    signal,
  });
  const data = await parse(res);
  if (!res.ok) {
    throw new ApiError(data?.error?.message || '封面上传失败', { status: res.status });
  }
  return data.data;
}

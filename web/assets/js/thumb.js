// 从视频里截一帧当封面：不依赖 ffmpeg，用浏览器自身解码
// 只在「上传者本机」执行一次，随后把 JPEG 传到服务端，其他访客不会重复解码。

function waitEvent(el, ok, timeout = 10000) {
  return new Promise((resolve, reject) => {
    let done = false;
    const finish = (fn, arg) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      el.removeEventListener(ok, onOk);
      el.removeEventListener('error', onErr);
      fn(arg);
    };
    const onOk = () => finish(resolve);
    const onErr = () => finish(reject, new Error('视频解码失败'));
    const timer = setTimeout(() => finish(reject, new Error('视频解码超时')), timeout);
    el.addEventListener(ok, onOk, { once: true });
    el.addEventListener('error', onErr, { once: true });
  });
}

/**
 * @param {File|Blob|string} source 本地文件或可访问的视频地址
 * @param {{maxWidth?: number, at?: number, quality?: number}} opts
 * @returns {Promise<Blob>} JPEG
 */
export async function grabPoster(source, opts = {}) {
  const { maxWidth = 1280, at = 1.2, quality = 0.85 } = opts;
  const objectUrl = typeof source === 'string' ? null : URL.createObjectURL(source);
  const url = objectUrl || source;
  const video = document.createElement('video');
  video.preload = 'auto';
  video.muted = true;
  video.playsInline = true;
  video.crossOrigin = 'anonymous';
  video.src = url;

  try {
    await waitEvent(video, 'loadedmetadata');
    const duration = Number.isFinite(video.duration) && video.duration > 0 ? video.duration : 2;
    const target = Math.min(at, Math.max(0.05, duration * 0.2), Math.max(0.05, duration - 0.05));
    try {
      video.currentTime = target;
      await waitEvent(video, 'seeked', 6000);
    } catch {
      // 某些容器不支持精确 seek，退化为首帧
    }

    const vw = video.videoWidth || 1280;
    const vh = video.videoHeight || 720;
    const w = Math.min(maxWidth, vw);
    const h = Math.max(2, Math.round((w * vh) / vw / 2) * 2);

    const canvas = document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(video, 0, 0, w, h);

    const blob = await new Promise((resolve, reject) => {
      canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('生成封面失败'))), 'image/jpeg', quality);
    });
    return blob;
  } finally {
    video.removeAttribute('src');
    try { video.load(); } catch { /* ignore */ }
    if (objectUrl) URL.revokeObjectURL(objectUrl);
  }
}

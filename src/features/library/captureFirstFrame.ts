import { convertFileSrc, isTauri } from '@tauri-apps/api/core';

const WIDTH = 176;
const HEIGHT = 99;
const MAX_ACTIVE = 2;
let active = 0;
const waiting: Array<() => void> = [];

function limited<T>(job: () => Promise<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    const run = () => {
      active += 1;
      void job().then(resolve, reject).finally(() => {
        active -= 1;
        waiting.shift()?.();
      });
    };
    if (active < MAX_ACTIVE) run();
    else waiting.push(run);
  });
}

/** 抓取真实第一帧；失败只影响缩略图，不影响视频或改名功能。 */
export function captureFirstFrame(path: string, signal: AbortSignal): Promise<string | null> {
  if (!isTauri() || signal.aborted) return Promise.resolve(null);
  return limited(() => {
    if (signal.aborted) return Promise.resolve(null);
    return new Promise<string | null>(resolve => {
      const video = document.createElement('video');
      let finished = false;
      const finish = (image: string | null) => {
        if (finished) return;
        finished = true;
        window.clearTimeout(timer);
        signal.removeEventListener('abort', onAbort);
        video.removeEventListener('loadeddata', onReady);
        video.removeEventListener('error', onError);
        video.removeAttribute('src');
        video.load();
        resolve(image);
      };
      const onAbort = () => finish(null);
      const onError = () => finish(null);
      const onReady = () => {
        if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !video.videoWidth || !video.videoHeight) {
          finish(null); return;
        }
        try {
          const canvas = document.createElement('canvas');
          canvas.width = WIDTH;
          canvas.height = HEIGHT;
          const context = canvas.getContext('2d');
          if (!context) { finish(null); return; }
          context.fillStyle = '#161b17';
          context.fillRect(0, 0, WIDTH, HEIGHT);
          const scale = Math.min(WIDTH / video.videoWidth, HEIGHT / video.videoHeight);
          const width = video.videoWidth * scale;
          const height = video.videoHeight * scale;
          context.drawImage(video, (WIDTH - width) / 2, (HEIGHT - height) / 2, width, height);
          const image = canvas.toDataURL('image/jpeg', 0.78);
          finish(image.startsWith('data:image/jpeg;base64,') ? image : null);
        } catch { finish(null); }
      };
      const timer = window.setTimeout(() => finish(null), 10_000);
      signal.addEventListener('abort', onAbort, { once: true });
      video.addEventListener('loadeddata', onReady, { once: true });
      video.addEventListener('error', onError, { once: true });
      video.crossOrigin = 'anonymous';
      video.preload = 'auto';
      video.muted = true;
      video.playsInline = true;
      try { video.src = convertFileSrc(path); video.load(); }
      catch { finish(null); }
    });
  });
}

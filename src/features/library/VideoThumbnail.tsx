import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getThumbnail, saveThumbnail } from '../../lib/desktop';
import type { Video } from '../../lib/types';
import { captureFirstFrame } from './captureFirstFrame';

export function VideoThumbnail({ video }: { video: Video }) {
  const element = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false);
  const [image, setImage] = useState<string | null>(null);

  useEffect(() => {
    if (video.status !== 'ready' || !isTauri()) return;
    const target = element.current;
    if (!target || !('IntersectionObserver' in window)) { setVisible(true); return; }
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) { setVisible(true); observer.disconnect(); }
    }, { root: target.closest('.file-list'), rootMargin: '180px' });
    observer.observe(target);
    return () => observer.disconnect();
  }, [video.id, video.status]);

  useEffect(() => {
    if (!visible || video.status !== 'ready') return;
    const controller = new AbortController();
    let active = true;
    void (async () => {
      try {
        const cached = await getThumbnail(video.id);
        if (!active || controller.signal.aborted) return;
        if (cached) { setImage(cached); return; }
        const captured = await captureFirstFrame(video.currentPath, controller.signal);
        if (!active || !captured) return;
        setImage(captured);
        void saveThumbnail(video.id, captured).catch(() => { /* 磁盘缓存失败不影响当前画面 */ });
      } catch { /* 无法解码或读取时显示占位图，不影响文件操作 */ }
    })();
    return () => { active = false; controller.abort(); };
  }, [visible, video.id, video.currentPath, video.status]);

  return <span ref={element} className="video-thumb" aria-hidden="true">
    {image && video.status === 'ready' ? <img src={image} alt="" onError={() => setImage(null)} /> : <span className="video-thumb-placeholder">▶</span>}
  </span>;
}

import { afterEach, describe, expect, it, vi } from 'vitest';
import { captureFirstFrame } from './captureFirstFrame';

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => true,
  convertFileSrc: (path: string) => `https://asset.localhost/${encodeURIComponent(path)}`,
}));

afterEach(() => vi.restoreAllMocks());

function videoFixture() {
  let video: HTMLVideoElement | null = null;
  const original = document.createElement.bind(document);
  vi.spyOn(document, 'createElement').mockImplementation(((name: string) => {
    const element = original(name);
    if (name === 'video') video = element as HTMLVideoElement;
    return element;
  }) as typeof document.createElement);
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {});
  return { current: () => { if (!video) throw new Error('video was not requested'); return video; } };
}

describe('视频首帧缩略图', () => {
  it('等到真实首帧加载后才绘制并输出小尺寸 JPEG', async () => {
    const fixture = videoFixture();
    const draw = vi.fn();
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(() => ({
      fillRect: vi.fn(), drawImage: draw, fillStyle: '',
    }) as unknown as CanvasRenderingContext2D);
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue('data:image/jpeg;base64,/9j/AA==');
    const result = captureFirstFrame('C:\\测试\\原片.mp4', new AbortController().signal);
    const video = fixture.current();
    Object.defineProperties(video, {
      readyState: { value: HTMLMediaElement.HAVE_CURRENT_DATA },
      videoWidth: { value: 1920 }, videoHeight: { value: 1080 },
    });
    video.dispatchEvent(new Event('loadeddata'));
    expect(await result).toBe('data:image/jpeg;base64,/9j/AA==');
    expect(draw).toHaveBeenCalledOnce();
    expect(draw.mock.calls[0]?.[0]).toBe(video);
    expect(video.hasAttribute('src')).toBe(false);
  });
  it('格式不受支持时返回空图，不影响文件操作', async () => {
    const fixture = videoFixture();
    const result = captureFirstFrame('/测试/损坏.mkv', new AbortController().signal);
    fixture.current().dispatchEvent(new Event('error'));
    expect(await result).toBeNull();
  });
  it('滚动离开导致请求取消时，停止等待并释放视频', async () => {
    const fixture = videoFixture();
    const controller = new AbortController();
    const result = captureFirstFrame('/测试/很大.mp4', controller.signal);
    const video = fixture.current();
    controller.abort();
    expect(await result).toBeNull();
    expect(video.hasAttribute('src')).toBe(false);
  });
});

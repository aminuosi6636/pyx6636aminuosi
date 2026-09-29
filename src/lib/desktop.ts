import { invoke, isTauri } from '@tauri-apps/api/core';
import { error } from '@tauri-apps/plugin-log';
import type { BootstrapReport, LibraryStatus } from './types';
export function hasDesktopRuntime(): boolean { return isTauri(); }
export function loadBootstrapReport(): Promise<BootstrapReport> { return invoke('startup_health'); }
export function getLibraryStatus(page = 0): Promise<LibraryStatus> { return invoke('library_status', { page }); }
export function chooseLibrary(): Promise<LibraryStatus | null> { return invoke('choose_root'); }
export function rescanLibrary(): Promise<LibraryStatus> { return invoke('rescan_library'); }
export function setUsage(videoId: string, expectedCount: number, count: number, requestId: string): Promise<void> {
  return invoke('set_usage', { videoId, expectedCount, count, requestId });
}
export function getThumbnail(videoId: string): Promise<string | null> { return invoke('thumbnail_data', { videoId }); }
export function saveThumbnail(videoId: string, dataUrl: string): Promise<void> { return invoke('save_thumbnail', { videoId, dataUrl }); }
export function reportFrontendError(value: unknown): void {
  const detail = value instanceof Error ? `${value.message}\n${value.stack ?? ''}` : String(value);
  if (isTauri()) void error(`frontend: ${detail}`).catch(() => console.error(detail));
  else console.error(detail);
}

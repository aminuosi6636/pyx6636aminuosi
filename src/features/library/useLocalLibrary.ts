import { useCallback, useEffect, useRef, useState } from 'react';
import { chooseLibrary, getLibraryStatus, rescanLibrary, reportFrontendError, setUsage } from '../../lib/desktop';
import { toAppError } from '../../lib/errors';
import type { AppError, LibraryStatus, Video } from '../../lib/types';
export function useLocalLibrary() {
  const [library, setLibrary] = useState<LibraryStatus | null>(null);
  const [issue, setIssue] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [page, setPage] = useState(0);
  const lock = useRef(false);
  const generation = useRef(0);
  useEffect(() => {
    let active = true; let polling = false;
    const refresh = async () => {
      if (polling || lock.current) return;
      polling = true; const version = generation.current;
      try { const next = await getLibraryStatus(page); if (active && version === generation.current) setLibrary(next); }
      catch (error: unknown) { if (active) setIssue(toAppError(error)); }
      finally { polling = false; }
    };
    void refresh(); const timer = window.setInterval(() => { void refresh(); }, 1000);
    return () => { active = false; window.clearInterval(timer); };
  }, [page]);
  const perform = useCallback(async (action: () => Promise<LibraryStatus | null>) => {
    if (lock.current) return;
    lock.current = true; generation.current += 1; setBusy(true); setIssue(null);
    try { const next = await action(); if (next) setLibrary(next); }
    catch (error: unknown) { reportFrontendError(error); setIssue(toAppError(error)); }
    finally { lock.current = false; setBusy(false); }
  }, []);
  const applyCount = (video: Video, count: number) => perform(async () => {
    await setUsage(video.id, video.useCount, count, crypto.randomUUID());
    return getLibraryStatus(page);
  });
  return { library, issue, busy, page, setPage, applyCount,
    choose: () => perform(async () => { const next = await chooseLibrary(); if (next) setPage(0); return next; }),
    scan: () => perform(async () => { await rescanLibrary(); return getLibraryStatus(page); }) };
}

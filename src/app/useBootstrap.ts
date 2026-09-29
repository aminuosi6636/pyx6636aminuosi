import { useEffect, useState } from 'react';
import { hasDesktopRuntime, loadBootstrapReport, reportFrontendError } from '../lib/desktop';
import { toAppError } from '../lib/errors';
import type { AppError, BootstrapReport } from '../lib/types';

type BootstrapState =
  | { kind: 'browser' }
  | { kind: 'loading' }
  | { kind: 'ready'; report: BootstrapReport }
  | { kind: 'failed'; issue: AppError };

export function useBootstrap(): BootstrapState {
  const [state, setState] = useState<BootstrapState>({ kind: 'loading' });
  useEffect(() => {
    let active = true;
    if (!hasDesktopRuntime()) { setState({ kind: 'browser' }); return; }
    void loadBootstrapReport().then(report => {
      if (active) setState({ kind: 'ready', report });
    }).catch((error: unknown) => {
      reportFrontendError(error);
      if (active) setState({ kind: 'failed', issue: toAppError(error) });
    });
    return () => { active = false; };
  }, []);
  return state;
}

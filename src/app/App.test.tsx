import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import type { BootstrapReport } from '../lib/types';
import { hasDesktopRuntime, loadBootstrapReport, reportFrontendError, getLibraryStatus } from '../lib/desktop';

vi.mock('../lib/desktop', () => ({ hasDesktopRuntime: vi.fn(), loadBootstrapReport: vi.fn(), reportFrontendError: vi.fn(), getLibraryStatus: vi.fn(), chooseLibrary: vi.fn(), rescanLibrary: vi.fn() }));
const healthy: BootstrapReport = { appVersion: '0.1.0', databaseVersion: 1, databaseReady: true, dataDirectory: '/local/data', logDirectory: '/local/logs', issue: null };

describe('启动界面', () => {
  beforeEach(() => vi.mocked(getLibraryStatus).mockResolvedValue({id:null,rootPath:null,scanning:false,total:0,ready:0,issue:null,candidates:[],videos:[],page:0}));
  it('浏览器不会伪装连接成功，也不访问数据库', () => {
    vi.mocked(hasDesktopRuntime).mockReturnValue(false);
    render(<App />);
    expect(screen.getByText('请在桌面应用中打开素材库')).toBeInTheDocument();
    expect(loadBootstrapReport).not.toHaveBeenCalled();
  });
  it('数据库就绪后才开放原生目录选择', async () => {
    vi.mocked(hasDesktopRuntime).mockReturnValue(true);
    vi.mocked(loadBootstrapReport).mockResolvedValue(healthy);
    render(<App />);
    expect(await screen.findByRole('button',{name:'选择素材根目录'})).toBeEnabled();
  });
  it('数据库损坏显示停止状态与可理解的中文错误', async () => {
    vi.mocked(hasDesktopRuntime).mockReturnValue(true);
    vi.mocked(loadBootstrapReport).mockResolvedValue({ ...healthy, databaseReady: false, issue: { code: 'DATABASE', message: '本地数据库未能打开，请联系维护人员检查日志。', retryable: false } });
    render(<App />);
    expect(await screen.findByText('本地数据库需要检查')).toBeInTheDocument();
    expect(screen.queryByText('本地视频已就绪')).not.toBeInTheDocument();
  });
  it('IPC 异常不泄露原始技术错误，并记录日志', async () => {
    vi.mocked(hasDesktopRuntime).mockReturnValue(true);
    vi.mocked(loadBootstrapReport).mockRejectedValue(new Error('SQLITE_CORRUPT raw internal path'));
    render(<App />);
    expect(await screen.findByText('启动检查未完成')).toBeInTheDocument();
    expect(screen.queryByText(/SQLITE_CORRUPT/)).not.toBeInTheDocument();
    expect(reportFrontendError).toHaveBeenCalledOnce();
  });
});



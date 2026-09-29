import { useBootstrap } from './useBootstrap';
import { SetupPanel } from '../features/setup/SetupPanel';
import { LibraryPanel } from '../features/library/LibraryPanel';
export function App() {
  const state = useBootstrap();
  return <div className="app-shell"><main className="main-content">
    <header className="topbar"><div className="brand"><span className="brand-mark" aria-hidden="true">▣</span><div><h1>混剪素材整理助手</h1><p>自动日期命名 · 直接填写使用次数</p></div></div><span className="local-badge">本地小工具</span></header>
    {state.kind === 'ready' && state.report.databaseReady ? <LibraryPanel /> : <SetupPanel state={state} />}
    <footer>新视频复制完成后自动命名为 日期_时-分-秒。同一秒重名时补序号，保留扩展名。</footer>
  </main></div>;
}

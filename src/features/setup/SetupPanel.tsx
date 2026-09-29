import type { useBootstrap } from '../../app/useBootstrap';

export function SetupPanel({ state }: { state: ReturnType<typeof useBootstrap> }) {
  if (state.kind === 'loading') return <section className="setup-panel" aria-live="polite"><h2>正在检查本地数据库…</h2></section>;
  if (state.kind === 'failed') return <section className="setup-panel" role="alert"><h2>启动检查未完成</h2><p>{state.issue.message}</p><span className="issue-code">诊断编号：{state.issue.code}</span></section>;
  if (state.kind === 'browser') return <section className="setup-panel">
    <span className="panel-icon" aria-hidden="true">▣</span><div className="eyebrow">开发界面预览</div>
    <h2>请在桌面应用中打开素材库</h2><p>此窗口仅用于检查界面。素材访问和数据库检查需要桌面程序。</p>
    <div className="notice">没有连接桌面运行环境，素材操作暂不可用。</div>
  </section>;
  const { report } = state;
  return <section className="setup-panel" aria-live="polite">
    <span className="panel-icon" aria-hidden="true">▣</span><div className="eyebrow">本地工作空间</div>
    <h2>{report.databaseReady ? '本地视频已就绪' : '本地数据库需要检查'}</h2>
    <p>{report.databaseReady ? '素材目录和本地数据库已就绪。' : report.issue?.message ?? '请联系维护人员查看本地日志。'}</p>
    {report.issue && <div className="notice" role="alert">{report.issue.message}<br /><small>诊断编号：{report.issue.code}</small></div>}
    <dl className="diagnostics"><div><dt>应用版本</dt><dd>{report.appVersion}</dd></div><div><dt>数据库版本</dt><dd>{report.databaseReady ? `v${report.databaseVersion}` : '未就绪'}</dd></div></dl>
    <details><summary>维护信息</summary><p>数据目录：{report.dataDirectory ?? '不可用'}</p><p>日志目录：{report.logDirectory ?? '不可用'}</p></details>
  </section>;
}


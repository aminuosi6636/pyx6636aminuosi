import { Component } from 'react';
import type { ErrorInfo, ReactNode } from 'react';
import { reportFrontendError } from '../lib/desktop';

export class ErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: Error, info: ErrorInfo) {
    reportFrontendError(new Error(`${error.message}\n${info.componentStack ?? ''}`));
  }
  render() {
    if (this.state.failed) return <main className="fatal" role="alert">
      <h1>界面暂时无法显示</h1><p>请重新打开软件。素材文件不会因为界面错误被删除。</p>
      <button onClick={() => window.location.reload()}>重新加载界面</button>
    </main>;
    return this.props.children;
  }
}

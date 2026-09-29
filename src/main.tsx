import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import { ErrorBoundary } from './app/ErrorBoundary';
import './styles/app.css';

const root = document.getElementById('root');
if (!root) throw new Error('应用挂载节点不存在');
createRoot(root).render(<StrictMode><ErrorBoundary><App /></ErrorBoundary></StrictMode>);

import { StrictMode, Component, ErrorInfo, ReactNode } from 'react';
import ReactDOM from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauriRuntime } from './tauriRuntime';
import './styles.css';
import App from './App';
import MainWindow from './pages/MainWindow';

/**
 * 用 location.hash 区分 Orb / 主面板（#main / #orb）。
 * 纯浏览器预览：无 Tauri 时默认主面板（避免 Orb 调窗口 API）；显式 #orb 仍可看小球 UI。
 */
function resolveIsOrb(): boolean {
  const h = (window.location.hash || '').replace(/^#/, '').split(/[?&]/)[0];
  if (h === 'main') return false;
  if (h === 'orb') return true;
  if (!isTauriRuntime()) return false;
  try {
    return getCurrentWindow().label === 'orb';
  } catch {
    return true;
  }
}

const isOrb = resolveIsOrb();

if (isOrb) {
  document.documentElement.style.background = 'transparent';
  document.body.style.background = 'transparent';
} else {
  document.documentElement.style.background = '#0a0a14';
  document.body.style.background = '#0a0a14';
}

class ErrorBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };

  static getDerivedStateFromError(error: Error) {
    return { error: `${error.message}\n${error.stack}` };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[Auto-Heart] Render error:', error, info);
  }

  render() {
    if (this.state.error) {
      return (
        <div style={{
          padding: 24, fontFamily: 'monospace', fontSize: 12,
          color: '#ff6b6b', background: '#0a0a14', height: '100vh',
          whiteSpace: 'pre-wrap', wordBreak: 'break-all', overflow: 'auto',
        }}>
          <div style={{ color: '#ff5050', fontWeight: 700, marginBottom: 12, fontSize: 14 }}>
            Auto-Heart 渲染错误
          </div>
          {this.state.error}
        </div>
      );
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ErrorBoundary>
      {isOrb ? <App /> : <MainWindow />}
    </ErrorBoundary>
  </StrictMode>,
);

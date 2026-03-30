import {
  StrictMode,
  Component,
  ErrorInfo,
  ReactNode,
} from 'react';
import ReactDOM from 'react-dom/client';
import './styles.css';
import App from './App';
import MainWindow from './pages/MainWindow';

/** 从 URL 读面板类型 */
function viewFromUrl(): 'orb' | 'main' | '' {
  const q = new URLSearchParams(window.location.search).get('view');
  if (q === 'main' || q === 'orb') return q;
  return '';
}

// 模块加载时同步解析（这个阶段 window.location.search 已可用）
const isOrb = viewFromUrl() === 'orb';
document.documentElement.dataset.view = isOrb ? 'orb' : 'main';
document.body.dataset.view = isOrb ? 'orb' : 'main';

// 在任何 React 代码执行前同步设置背景
if (isOrb) {
  document.documentElement.style.background = 'transparent';
  document.body.style.background = 'transparent';
  document.getElementById('root')?.style.setProperty('background', 'transparent');
} else {
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  const initialBackground = prefersDark ? '#0f172a' : '#f3f7fb';
  document.documentElement.style.background = initialBackground;
  document.body.style.background = initialBackground;
  document.getElementById('root')?.style.setProperty('background', initialBackground);
}

function Shell() {
  return isOrb ? <App /> : <MainWindow />;
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
      <Shell />
    </ErrorBoundary>
  </StrictMode>,
);

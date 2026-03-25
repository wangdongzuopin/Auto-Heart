import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { isTauriRuntime } from './tauriRuntime';
import Orb from './components/Orb';
import SpeechBubble from './components/SpeechBubble';
import './App.css';

const POS_KEY = 'auto-heart:orb-pos-v3';

/** WebView2 透明窗口对 alpha=0 区域常不做命中测试；铺极淡底色保证能收到指针事件 */
const HIT_BG = 'rgba(0, 0, 0, 0.03)';

interface IncomingMessage {
  id: string;
  title: string;
  content: string;
  priority: number;
}

type BubbleType = 'message' | 'report';

function App() {
  const [orbState, setOrbState] = useState<'idle' | 'thinking' | 'speaking'>('idle');
  const isDraggingRef = useRef(false);
  const [bubbleMessage, setBubbleMessage] = useState<{
    id: string;
    title: string;
    content: string;
    type: BubbleType;
  } | null>(null);

  const handleOrbClick = useCallback(async () => {
    try {
      await invoke('open_main_window');
    } catch (err) {
      console.error('Failed to open main window:', err);
    }
  }, []);

  /**
   * Windows 透明浮窗：仅靠 data-tauri-drag-region 往往无效（透明像素不命中）。
   * 使用程序化 startDragging（需 capabilities 中 core:window:allow-start-dragging）。
   * 单击仍由 Orb 的 onClick 打开主窗口；此处不在 pointerup 里重复触发，避免与气泡按钮冲突。
   */
  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return;
    isDraggingRef.current = false;
    const startX = e.clientX;
    const startY = e.clientY;

    const cleanup = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
    };

    const onMove = (me: PointerEvent) => {
      if (isDraggingRef.current) return;
      const dx = Math.abs(me.clientX - startX);
      const dy = Math.abs(me.clientY - startY);
      if (dx > 4 || dy > 4) {
        isDraggingRef.current = true;
        cleanup();
        if (isTauriRuntime()) {
          getCurrentWindow()
            .startDragging()
            .catch((err) => console.error('[orb] startDragging:', err));
        }
      }
    };

    const onUp = () => {
      cleanup();
    };

    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
  }, []);

  const handleBubbleDismiss = async () => {
    if (bubbleMessage) {
      try {
        await invoke('dismiss_message', { id: bubbleMessage.id });
      } catch {}
    }
    setBubbleMessage(null);
    setOrbState('idle');
  };

  const handleBubbleAction = async () => {
    if (bubbleMessage) {
      if (bubbleMessage.type === 'report') {
        try { await invoke('open_main_window'); } catch {}
      } else {
        try { await invoke('ack_message', { id: bubbleMessage.id }); } catch {}
      }
    }
    setBubbleMessage(null);
    setOrbState('idle');
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const win = getCurrentWindow();

    const restorePosition = async () => {
      try {
        const saved = localStorage.getItem(POS_KEY);
        if (saved) {
          const { x, y } = JSON.parse(saved) as { x: number; y: number };
          await win.setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
        }
      } catch (e) {
        console.warn('[orb] position restore failed:', e);
      }
    };

    restorePosition();

    let unlistenMove: (() => void) | undefined;
    win.onMoved((event) => {
      localStorage.setItem(POS_KEY, JSON.stringify({ x: event.payload.x, y: event.payload.y }));
    }).then((fn) => { unlistenMove = fn; });

    return () => { unlistenMove?.(); };
  }, []);

  useEffect(() => {
    let unlistenNew: (() => void) | undefined;
    let unlistenMiddle: (() => void) | undefined;
    let unlistenDeep: (() => void) | undefined;
    let thinkingTimer: ReturnType<typeof setTimeout> | undefined;

    const setup = async () => {
      if (!isTauriRuntime()) return;

      unlistenMiddle = await listen('heartbeat:middle', () => {
        if (orbState !== 'speaking') {
          setOrbState('thinking');
          thinkingTimer = setTimeout(() => setOrbState('idle'), 3000);
        }
      });

      unlistenNew = await listen<IncomingMessage>('message_queue:new', (event) => {
        clearTimeout(thinkingTimer);
        const { id, title, content } = event.payload;
        setOrbState('speaking');
        setBubbleMessage({ id, title, content, type: 'message' });
      });

      unlistenDeep = await listen<{ date: string; preview: string }>('daily_report:ready', (event) => {
        setOrbState('speaking');
        setBubbleMessage({
          id: `report-${event.payload.date}`,
          title: '日报已生成',
          content: event.payload.preview,
          type: 'report',
        });
      });
    };

    setup();

    return () => {
      clearTimeout(thinkingTimer);
      unlistenNew?.();
      unlistenMiddle?.();
      unlistenDeep?.();
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [orbState]);

  return (
    <div
      onPointerDown={handlePointerDown}
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: HIT_BG,
        position: 'relative',
        touchAction: 'none',
      }}
    >
      <div style={{ position: 'relative' }}>
        <Orb onClick={handleOrbClick} state={orbState} size={120} />
        {bubbleMessage && (
          <SpeechBubble
            message={{ title: bubbleMessage.title, content: bubbleMessage.content }}
            onDismiss={handleBubbleDismiss}
            onAction={handleBubbleAction}
            actionLabel={bubbleMessage.type === 'report' ? '查看日报' : '帮我改'}
            dismissLabel={bubbleMessage.type === 'report' ? '稍后' : '忽略'}
          />
        )}
      </div>
    </div>
  );
}

export default App;

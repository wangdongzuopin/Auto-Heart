import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window';
import { isTauriRuntime } from '../tauriRuntime';
import TodayTab from './TodayTab';
import SemanticMapTab from './SemanticMapTab';
import SettingsTab from './SettingsTab';
import ConversationTab from './ConversationTab';

type Tab = 'today' | 'semantic-map' | 'settings' | 'conversation';

/**
 * MainWindow — Auto-Heart 主窗口面板
 * 
 * 结构匹配 prototype：
 * - 标题栏：Mini Orb + "Auto-Heart" + 活跃状态
 * - 三个 Tab：今天 / 语义地图 / 设置
 * - 内容区
 */
export default function MainWindow() {
  // ── 窗口位置缓存 ──
  useEffect(() => {
    if (!isTauriRuntime()) return;

    const restorePosition = async () => {
      try {
        const state = await invoke<{ x: number; y: number; width: number; height: number; is_maximized: boolean } | null>('load_window_state');
        if (state) {
          const win = getCurrentWindow();
          await win.setPosition(new PhysicalPosition(state.x, state.y));
          if (state.is_maximized) {
            await win.maximize();
          }
        }
      } catch (e) {
        console.warn('[MainWindow] restore position failed:', e);
      }
    };

    restorePosition();
  }, []);

  // 关闭窗口时保存位置
  useEffect(() => {
    if (!isTauriRuntime()) return;

    let prevented = false;

    const handleClose = async () => {
      try {
        const win = getCurrentWindow();
        const pos = await win.outerPosition();
        const size = await win.outerSize();
        await invoke('save_window_state', {
          state: {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            is_maximized: await win.isMaximized(),
          },
        });
      } catch (e) {
        console.warn('[MainWindow] save position failed:', e);
      }
    };

    // 监听窗口关闭事件
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      if (prevented) return;
      prevented = true;
      event.preventDefault();
      await handleClose();
      await invoke('close_main_window');
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // 监听导航事件
  useEffect(() => {
    if (!isTauriRuntime()) return;

    let unlisten: (() => void) | undefined;

    const setup = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      unlisten = await listen<string>('navigate_to', (event) => {
        const tab = event.payload as Tab;
        if (['today', 'semantic-map', 'conversation', 'settings'].includes(tab)) {
          setActiveTab(tab);
        }
      });
    };

    setup();

    return () => {
      unlisten?.();
    };
  }, []);

  // 从 URL 参数获取初始 Tab
const getInitialTab = (): Tab => {
  if (!isTauriRuntime()) return 'today';
  const params = new URLSearchParams(window.location.search);
  const view = params.get('view');
  if (view === 'conversation') return 'conversation';
  if (view === 'semantic-map') return 'semantic-map';
  if (view === 'settings') return 'settings';
  return 'today';
};

const [activeTab, setActiveTab] = useState<Tab>(getInitialTab);

  const tabs: { id: Tab; label: string }[] = [
    { id: 'today', label: '今天' },
    { id: 'semantic-map', label: '语义地图' },
    { id: 'conversation', label: '对话' },
    { id: 'settings', label: '设置' },
  ];

  return (
    <div
      style={{
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        background: 'var(--color-background-primary, #0a0a14)',
        color: 'var(--color-text-primary, #eeeeee)',
        fontFamily: 'var(--font-sans, system-ui, sans-serif)',
      }}
    >
      {/* 标题栏 */}
      <div
        style={{
          padding: '10px 16px',
          borderBottom: '0.5px solid var(--color-border-tertiary)',
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          background: 'var(--color-background-secondary)',
        }}
      >
        {/* Mini Orb */}
        <div
          style={{
            width: 20,
            height: 20,
            borderRadius: '50%',
            background: 'var(--color-brand-light)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <div
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              background: 'var(--color-brand)',
            }}
          />
        </div>
        <span style={{ fontSize: 13, fontWeight: 500 }}>Auto-Heart</span>
        <span
          style={{
            marginLeft: 'auto',
            fontSize: 11,
            color: 'var(--color-text-success)',
            display: 'flex',
            alignItems: 'center',
            gap: 4,
          }}
        >
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: '50%',
              background: 'currentColor',
              display: 'inline-block',
            }}
          />
          活跃
        </span>

        {/* 关闭按钮 */}
        <button
          onClick={async () => {
            if (!isTauriRuntime()) return;
            try {
              // 先保存窗口位置
              const win = getCurrentWindow();
              const pos = await win.outerPosition();
              const size = await win.outerSize();
              await invoke('save_window_state', {
                state: {
                  x: pos.x,
                  y: pos.y,
                  width: size.width,
                  height: size.height,
                  is_maximized: await win.isMaximized(),
                },
              });
              // 再关闭
              await invoke('close_main_window');
            } catch (e) {
              console.error('[MainWindow] close:', e);
            }
          }}
          style={{
            marginLeft: 8,
            width: 22,
            height: 22,
            borderRadius: '50%',
            border: 'none',
            background: 'transparent',
            color: 'var(--color-text-tertiary)',
            cursor: 'pointer',
            fontSize: 14,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            transition: 'background 0.15s, color 0.15s',
          }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background = 'rgba(255,80,80,0.15)';
            (e.currentTarget as HTMLButtonElement).style.color = '#ff5050';
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background = 'transparent';
            (e.currentTarget as HTMLButtonElement).style.color = '#666';
          }}
          title="关闭"
        >
          ✕
        </button>
      </div>

      {/* Tab 导航 */}
      <div
        style={{
          display: 'flex',
          borderBottom: '0.5px solid var(--color-border-tertiary)',
        }}
      >
        {tabs.map((tab) => (
          <div
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            style={{
              padding: '8px 16px',
              fontSize: 12,
              color: activeTab === tab.id ? 'var(--color-brand)' : 'var(--color-text-tertiary)',
              borderBottom: activeTab === tab.id
                ? '2px solid var(--color-brand)'
                : '2px solid transparent',
              cursor: 'pointer',
              transition: 'color 0.15s, border-color 0.15s',
            }}
          >
            {tab.label}
          </div>
        ))}
      </div>

      {/* 内容区 */}
      <div style={{ flex: 1, overflow: 'auto' }}>
        {activeTab === 'today' && <TodayTab />}
        {activeTab === 'semantic-map' && <SemanticMapTab />}
        {activeTab === 'conversation' && <ConversationTab />}
        {activeTab === 'settings' && <SettingsTab />}
      </div>
    </div>
  );
}

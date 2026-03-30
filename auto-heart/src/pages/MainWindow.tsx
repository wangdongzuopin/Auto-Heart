import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { isTauriRuntime } from '../tauriRuntime';
import TodayTab from './TodayTab';
import SemanticMapTab from './SemanticMapTab';
import SettingsTab from './SettingsTab';
import ConversationTab from './ConversationTab';
import { useResolvedTheme } from '../hooks/useResolvedTheme';
import { useSettings } from '../hooks/useSettings';

type Tab = 'today' | 'semantic-map' | 'settings' | 'conversation';

const getInitialTab = (): Tab => {
  if (!isTauriRuntime()) return 'today';
  const params = new URLSearchParams(window.location.search);
  const view = params.get('view');
  if (view === 'conversation') return 'conversation';
  if (view === 'semantic-map') return 'semantic-map';
  if (view === 'settings') return 'settings';
  return 'today';
};

export default function MainWindow() {
  const [activeTab, setActiveTab] = useState<Tab>(getInitialTab);
  const { resolvedTheme } = useResolvedTheme();
  const { settings, updateSettings } = useSettings();

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const restoreWindowState = async () => {
      try {
        const state = await invoke<{ x: number; y: number; is_maximized: boolean } | null>('load_window_state');
        if (!state) return;
        const win = getCurrentWindow();
        await win.setPosition(new PhysicalPosition(state.x, state.y));
        if (state.is_maximized) await win.maximize();
      } catch (error) {
        console.warn('[MainWindow] restore state failed:', error);
      }
    };

    restoreWindowState();
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    let unlisten: (() => void) | undefined;

    const setup = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      unlisten = await listen<string>('navigate_to', (event) => {
        const nextTab = event.payload as Tab;
        if (['today', 'semantic-map', 'conversation', 'settings'].includes(nextTab)) {
          setActiveTab(nextTab);
        }
      });
    };

    setup();
    return () => unlisten?.();
  }, []);

  const saveWindowState = async () => {
    if (!isTauriRuntime()) return;
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
    } catch (error) {
      console.warn('[MainWindow] save state failed:', error);
    }
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;

    let prevented = false;
    const promise = getCurrentWindow().onCloseRequested(async (event) => {
      if (prevented) return;
      prevented = true;
      event.preventDefault();
      await saveWindowState();
      await invoke('close_main_window');
    });

    return () => {
      promise.then((fn) => fn());
    };
  }, []);

  const cycleThemeMode = async () => {
    const order: Array<'system' | 'light' | 'dark'> = ['system', 'light', 'dark'];
    const currentIndex = order.indexOf(settings.themeMode);
    const nextTheme = order[(currentIndex + 1) % order.length];
    await updateSettings({ themeMode: nextTheme });
  };

  const tabs: Array<{ id: Tab; label: string }> = [
    { id: 'today', label: '今天' },
    { id: 'semantic-map', label: '语义地图' },
    { id: 'conversation', label: '对话' },
    { id: 'settings', label: '设置' },
  ];

  return (
    <div style={{ height: '100vh', display: 'flex', flexDirection: 'column', background: 'var(--color-background-primary)', color: 'var(--color-text-primary)', fontFamily: 'var(--font-sans)' }}>
      <div style={{ padding: '10px 16px', borderBottom: '0.5px solid var(--color-border-tertiary)', display: 'flex', alignItems: 'center', gap: 10, background: 'var(--color-background-secondary)' }}>
        <div style={{ width: 20, height: 20, borderRadius: '50%', background: 'var(--color-brand-light)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <div style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--color-brand)' }} />
        </div>
        <span style={{ fontSize: 13, fontWeight: 600 }}>Auto-Heart</span>
        <button
          onClick={cycleThemeMode}
          style={{ padding: '3px 8px', fontSize: 10, borderRadius: 999, border: '0.5px solid var(--color-border-primary)', background: 'var(--color-background-tertiary)', color: 'var(--color-text-secondary)', cursor: 'pointer' }}
          title="切换主题"
        >
          {settings.themeMode === 'system'
            ? `跟随系统 · ${resolvedTheme === 'dark' ? '暗' : '亮'}`
            : settings.themeMode === 'dark'
              ? '暗色模式'
              : '明亮模式'}
        </button>
        <span style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--color-text-success)', display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{ width: 5, height: 5, borderRadius: '50%', background: 'currentColor', display: 'inline-block' }} />
          活跃
        </span>
        <button
          onClick={async () => {
            await saveWindowState();
            await invoke('close_main_window');
          }}
          style={{ width: 24, height: 24, borderRadius: '50%', border: 'none', background: 'transparent', color: 'var(--color-text-tertiary)', cursor: 'pointer' }}
          title="关闭"
        >
          ×
        </button>
      </div>

      <div style={{ display: 'flex', borderBottom: '0.5px solid var(--color-border-tertiary)' }}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            style={{
              padding: '9px 16px',
              fontSize: 12,
              cursor: 'pointer',
              background: 'transparent',
              border: 'none',
              borderBottom: activeTab === tab.id ? '2px solid var(--color-brand)' : '2px solid transparent',
              color: activeTab === tab.id ? 'var(--color-brand)' : 'var(--color-text-tertiary)',
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div style={{ flex: 1, overflow: 'auto' }}>
        {activeTab === 'today' && <TodayTab />}
        {activeTab === 'semantic-map' && <SemanticMapTab />}
        {activeTab === 'conversation' && <ConversationTab />}
        {activeTab === 'settings' && <SettingsTab />}
      </div>
    </div>
  );
}

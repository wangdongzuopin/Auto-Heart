import { useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauriRuntime } from '../tauriRuntime';
import TodayTab from './TodayTab';
import SemanticMapTab from './SemanticMapTab';
import SettingsTab from './SettingsTab';

type Tab = 'today' | 'semantic-map' | 'settings';

/**
 * MainWindow — Auto-Heart 主窗口面板
 * 
 * 结构匹配 prototype：
 * - 标题栏：Mini Orb + "Auto-Heart" + 活跃状态
 * - 三个 Tab：今天 / 语义地图 / 设置
 * - 内容区
 */
export default function MainWindow() {
  const [activeTab, setActiveTab] = useState<Tab>('today');

  const tabs: { id: Tab; label: string }[] = [
    { id: 'today', label: '今天' },
    { id: 'semantic-map', label: '语义地图' },
    { id: 'settings', label: '设置' },
  ];

  return (
    <div
      style={{
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        background: 'var(--color-background-primary)',
        color: 'var(--color-text-primary)',
        fontFamily: 'var(--font-sans)',
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
          onClick={() => {
            if (isTauriRuntime()) getCurrentWindow().close();
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
        {activeTab === 'settings' && <SettingsTab />}
      </div>
    </div>
  );
}

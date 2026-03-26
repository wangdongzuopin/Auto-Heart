import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useSettings, Settings } from '../hooks/useSettings';

// ──────────────────────────────────────────────
// 模型配置注册表
// ──────────────────────────────────────────────

interface ModelProvider {
  id: string;
  name: string;
  color: string;
  keyField: keyof Settings | null;
  models: string[];
  hint: string;
}

const PROVIDERS: ModelProvider[] = [
  { id: 'kimi',       name: 'Kimi',       color: '#1A73E8', keyField: 'kimiApiKey',       models: ['moonshot-v1-8k', 'moonshot-v1-32k', 'moonshot-v1-128k'], hint: '月之暗面 · 超长上下文' },
  { id: 'qwen',       name: 'Qwen',       color: '#FF6200', keyField: 'qwenApiKey',        models: ['qwen-turbo', 'qwen-plus', 'qwen-max', 'qwen-long'],       hint: '通义千问 · 阿里云' },
  { id: 'minimax',    name: 'MiniMax',    color: '#6C5CE7', keyField: 'minimaxApiKey',     models: ['abab6.5s-chat', 'abab6.5-chat'],                           hint: 'MiniMax · 低延迟' },
  { id: 'gpt',        name: 'GPT',        color: '#10A37F', keyField: 'gptApiKey',         models: ['gpt-4o-mini', 'gpt-4o', 'o1-mini', 'gpt-4-turbo'],        hint: 'OpenAI · 通用强模型' },
  { id: 'claude',     name: 'Claude',     color: '#E16B1A', keyField: 'claudeApiKey',      models: ['claude-haiku-4-5', 'claude-sonnet-4-5', 'claude-opus-4-5'], hint: 'Anthropic · 长文本' },
  { id: 'deepseek',   name: 'DeepSeek',   color: '#0066FF', keyField: 'deepseekApiKey',    models: ['deepseek-chat', 'deepseek-reasoner'],                      hint: 'DeepSeek · 高性价比' },
  { id: 'openrouter', name: 'OpenRouter', color: '#8B5CF6', keyField: 'openrouterApiKey',  models: ['openai/gpt-4o-mini', 'anthropic/claude-sonnet-4-5', 'google/gemini-2.0-flash-001', 'deepseek/deepseek-chat'], hint: '统一代理 · 100+ 模型' },
  { id: 'ollama',     name: '本地',        color: '#555',    keyField: null,                models: [],                                                          hint: 'Ollama · 完全离线' },
];

const KEY_LABELS: Partial<Record<keyof Settings, string>> = {
  kimiApiKey: 'Kimi API Key',
  qwenApiKey: 'DashScope API Key',
  minimaxApiKey: 'MiniMax API Key',
  gptApiKey: 'OpenAI API Key',
  claudeApiKey: 'Anthropic API Key',
  deepseekApiKey: 'DeepSeek API Key',
  openrouterApiKey: 'OpenRouter API Key',
};

const KEY_PLACEHOLDERS: Partial<Record<keyof Settings, string>> = {
  kimiApiKey: 'sk-...',
  qwenApiKey: 'sk-...',
  minimaxApiKey: 'eyJ...',
  gptApiKey: 'sk-...',
  claudeApiKey: 'sk-ant-...',
  deepseekApiKey: 'sk-...',
  openrouterApiKey: 'sk-or-...',
};

// ──────────────────────────────────────────────
// 单层模型配置组件
// ──────────────────────────────────────────────

interface LayerModelPickerProps {
  label: string;
  hint: string;
  selectedProvider: string;
  selectedModelName: string;
  settings: Settings;
  onProviderChange: (provider: string) => void;
  onModelNameChange: (name: string) => void;
  onKeyChange: (keyField: keyof Settings, value: string) => void;
}

function LayerModelPicker({
  label, hint,
  selectedProvider, selectedModelName,
  settings,
  onProviderChange, onModelNameChange, onKeyChange,
}: LayerModelPickerProps) {
  const provider = PROVIDERS.find(p => p.id === selectedProvider) ?? PROVIDERS[0];
  const keyField = provider.keyField;
  const currentKey = keyField ? (settings[keyField] as string) : '';
  const isConfigured = provider.id === 'ollama' || !!currentKey;

  const inputStyle: React.CSSProperties = {
    width: '100%', padding: '7px 10px', fontSize: 12,
    background: 'var(--color-background-tertiary)',
    border: '0.5px solid var(--color-border-primary)',
    borderRadius: 6, color: 'var(--color-text-primary)',
    outline: 'none', boxSizing: 'border-box', transition: 'border-color 0.15s',
  };

  return (
    <div style={{ background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-lg)', padding: 14 }}>
      {/* 标题 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <span style={{ fontSize: 12, fontWeight: 500, color: 'var(--color-text-primary)' }}>{label}</span>
        <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>{hint}</span>
        {isConfigured && (
          <span style={{ marginLeft: 'auto', fontSize: 9, color: 'var(--color-text-success)', background: 'rgba(76,175,80,0.12)', padding: '1px 6px', borderRadius: 4 }}>已配置</span>
        )}
      </div>

      {/* 提供商卡片行 */}
      <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap', marginBottom: 12 }}>
        {PROVIDERS.map(p => {
          const pKey = p.keyField ? (settings[p.keyField] as string) : 'ok';
          const pConfigured = p.id === 'ollama' || !!pKey;
          const selected = p.id === selectedProvider;
          return (
            <button
              key={p.id}
              onClick={() => onProviderChange(p.id)}
              style={{
                padding: '5px 10px', fontSize: 11, borderRadius: 6, cursor: 'pointer',
                border: selected ? `1px solid ${p.color}` : '0.5px solid var(--color-border-primary)',
                background: selected ? `${p.color}18` : 'transparent',
                color: selected ? p.color : 'var(--color-text-tertiary)',
                transition: 'all 0.15s', position: 'relative',
              }}
            >
              {p.name}
              {pConfigured && (
                <span style={{
                  position: 'absolute', top: -3, right: -3,
                  width: 6, height: 6, borderRadius: '50%',
                  background: 'var(--color-text-success)',
                  border: '1px solid var(--color-background-secondary)',
                }} />
              )}
            </button>
          );
        })}
      </div>

      {/* 选中提供商的配置面板 */}
      <div style={{ background: 'var(--color-background-tertiary)', borderRadius: 8, padding: 12, display: 'flex', flexDirection: 'column', gap: 8 }}>
        <div style={{ fontSize: 10, color: provider.color, marginBottom: 2, fontWeight: 500 }}>
          {provider.hint}
        </div>

        {/* API Key 输入（Ollama 除外）*/}
        {keyField && (
          <div>
            <label style={{ fontSize: 10, color: 'var(--color-text-tertiary)', display: 'block', marginBottom: 3 }}>
              {KEY_LABELS[keyField] ?? 'API Key'}
            </label>
            <input
              type="password"
              placeholder={KEY_PLACEHOLDERS[keyField] ?? 'sk-...'}
              value={currentKey}
              onChange={(e) => onKeyChange(keyField, e.target.value)}
              onBlur={(e) => onKeyChange(keyField, e.target.value)}
              style={inputStyle}
              onFocus={(e) => { (e.target as HTMLInputElement).style.borderColor = provider.color; }}
            />
          </div>
        )}

        {/* Ollama：Base URL */}
        {provider.id === 'ollama' && (
          <div>
            <label style={{ fontSize: 10, color: 'var(--color-text-tertiary)', display: 'block', marginBottom: 3 }}>
              Ollama 服务地址
            </label>
            <input
              type="text"
              placeholder="http://localhost:11434"
              value={settings.ollamaBaseUrl}
              onChange={(e) => onKeyChange('ollamaBaseUrl', e.target.value)}
              style={inputStyle}
            />
          </div>
        )}

        {/* 模型版本选择 */}
        <div>
          <label style={{ fontSize: 10, color: 'var(--color-text-tertiary)', display: 'block', marginBottom: 3 }}>
            模型版本
          </label>
          {provider.models.length > 0 ? (
            <select
              value={selectedModelName || provider.models[0]}
              onChange={(e) => onModelNameChange(e.target.value)}
              style={{ ...inputStyle, cursor: 'pointer', appearance: 'none',
                backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23666'/%3E%3C/svg%3E")`,
                backgroundRepeat: 'no-repeat', backgroundPosition: 'right 10px center', paddingRight: 28,
              }}
            >
              {provider.models.map(m => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              placeholder={provider.id === 'ollama' ? 'qwen2.5:7b' : '自定义模型名'}
              value={selectedModelName}
              onChange={(e) => onModelNameChange(e.target.value)}
              style={inputStyle}
            />
          )}
        </div>
      </div>
    </div>
  );
}

// ──────────────────────────────────────────────
// 主页面
// ──────────────────────────────────────────────

export default function SettingsTab() {
  const { settings, updateSettings, loading } = useSettings();
  const [watchPathInput, setWatchPathInput] = useState('');
  const [saved, setSaved] = useState(false);
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);

  useEffect(() => {
    invoke<boolean>('plugin:autostart|is_enabled')
      .then((v) => setAutostartEnabled(v))
      .catch(() => {});
  }, []);

  const handleSave = async (patch: Partial<Settings>) => {
    await updateSettings(patch);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const toggleAutostart = async () => {
    setAutostartLoading(true);
    try {
      if (autostartEnabled) {
        await invoke('plugin:autostart|disable');
        setAutostartEnabled(false);
      } else {
        await invoke('plugin:autostart|enable');
        setAutostartEnabled(true);
      }
    } catch (e) { console.error(e); }
    setAutostartLoading(false);
  };

  const addWatchPath = () => {
    const trimmed = watchPathInput.trim();
    if (!trimmed || settings.watchPaths.includes(trimmed)) return;
    handleSave({ watchPaths: [...settings.watchPaths, trimmed] });
    setWatchPathInput('');
  };

  const removeWatchPath = (path: string) => {
    handleSave({ watchPaths: settings.watchPaths.filter((p) => p !== path) });
  };

  const inputStyle: React.CSSProperties = {
    width: '100%', padding: '8px 10px', fontSize: 12,
    background: 'var(--color-background-tertiary)',
    border: '0.5px solid var(--color-border-primary)',
    borderRadius: 6, color: 'var(--color-text-primary)',
    boxSizing: 'border-box', outline: 'none', transition: 'border-color 0.15s',
  };

  if (loading) {
    return <div style={{ padding: 16, fontSize: 12, color: 'var(--color-text-tertiary)' }}>加载设置中...</div>;
  }

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 20 }}>

      {saved && (
        <div style={{ fontSize: 11, color: 'var(--color-text-success)', background: 'rgba(76,175,80,0.1)', padding: '6px 10px', borderRadius: 6, textAlign: 'center' }}>
          ✓ 设置已保存
        </div>
      )}

      {/* ── 模型配置 ── */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 10 }}>模型配置</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <LayerModelPicker
            label="中层心跳"
            hint="每10分钟 · 语义理解 · 意图解析"
            selectedProvider={settings.middleModel}
            selectedModelName={settings.middleModelName}
            settings={settings}
            onProviderChange={(p) => handleSave({ middleModel: p, middleModelName: '' })}
            onModelNameChange={(m) => handleSave({ middleModelName: m })}
            onKeyChange={(f, v) => handleSave({ [f]: v } as Partial<Settings>)}
          />
          <LayerModelPicker
            label="深层心跳"
            hint="下班触发 · 日报生成 · 深度分析"
            selectedProvider={settings.deepModel}
            selectedModelName={settings.deepModelName}
            settings={settings}
            onProviderChange={(p) => handleSave({ deepModel: p, deepModelName: '' })}
            onModelNameChange={(m) => handleSave({ deepModelName: m })}
            onKeyChange={(f, v) => handleSave({ [f]: v } as Partial<Settings>)}
          />
        </div>
      </div>

      {/* ── 意图文档路径 ── */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 8 }}>意图文档路径</div>
        <input
          type="text"
          placeholder="支持 Notion / Obsidian / 本地 txt 路径"
          value={settings.intentDocPath}
          onChange={(e) => updateSettings({ intentDocPath: e.target.value })}
          onBlur={(e) => handleSave({ intentDocPath: e.target.value })}
          style={inputStyle}
          onFocus={(e) => { (e.target as HTMLInputElement).style.borderColor = '#534AB7'; }}
        />
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginTop: 4 }}>
          Auto-Heart 会监听此文件，解析你的工作计划
        </div>
      </div>

      {/* ── 感知范围 ── */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 8 }}>感知范围 · 项目目录</div>
        <div style={{ display: 'flex', gap: 6, marginBottom: 8 }}>
          <input
            type="text"
            placeholder="添加项目路径，如 D:/projects/my-app"
            value={watchPathInput}
            onChange={(e) => setWatchPathInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && addWatchPath()}
            style={{ ...inputStyle, flex: 1 }}
            onFocus={(e) => { (e.target as HTMLInputElement).style.borderColor = '#534AB7'; }}
            onBlur={(e) => { (e.target as HTMLInputElement).style.borderColor = '#333'; }}
          />
          <button
            onClick={addWatchPath}
            style={{ padding: '8px 14px', fontSize: 12, borderRadius: 6, cursor: 'pointer', background: 'var(--color-brand-light)', border: '0.5px solid var(--color-brand-ring)', color: 'var(--color-brand)' }}
          >添加</button>
        </div>
        {settings.watchPaths.length === 0 ? (
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', textAlign: 'center', padding: '10px', background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)' }}>
            尚未添加监听目录
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            {settings.watchPaths.map((p) => (
              <div key={p} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 10px', background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)' }}>
                <span style={{ fontSize: 11, color: 'var(--color-text-secondary)', flex: 1, wordBreak: 'break-all' }}>{p}</span>
                <span onClick={() => removeWatchPath(p)} style={{ fontSize: 11, color: 'var(--color-text-tertiary)', cursor: 'pointer', padding: '1px 6px', borderRadius: 3 }}
                  onMouseEnter={(e) => { (e.target as HTMLElement).style.color = '#ff5050'; }}
                  onMouseLeave={(e) => { (e.target as HTMLElement).style.color = '#666'; }}>✕</span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* ── 沉默阈值 ── */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 8 }}>沉默阈值</div>
        <div style={{ display: 'flex', gap: 6 }}>
          {([
            { id: 'focus' as const, label: '专注', desc: '仅致命问题才通知' },
            { id: 'normal' as const, label: '正常', desc: '重要提醒会通知' },
            { id: 'open' as const, label: '开放', desc: '建议与观察都通知' },
          ]).map((m) => (
            <button key={m.id} onClick={() => handleSave({ silenceMode: m.id })}
              style={{ flex: 1, padding: '8px 12px', fontSize: 11, borderRadius: 'var(--border-radius-md)', cursor: 'pointer', textAlign: 'left',
                border: settings.silenceMode === m.id ? '0.5px solid var(--color-brand)' : '0.5px solid var(--color-border-primary)',
                background: settings.silenceMode === m.id ? 'var(--color-brand-light)' : 'transparent',
                color: settings.silenceMode === m.id ? 'var(--color-brand)' : 'var(--color-text-tertiary)',
              }}
            >
              <div style={{ fontWeight: 500, marginBottom: 2 }}>{m.label}</div>
              <div style={{ fontSize: 10, opacity: 0.7 }}>{m.desc}</div>
            </button>
          ))}
        </div>
      </div>

      {/* ── 下班时间 ── */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 8 }}>下班时间</div>
        <input type="time" value={settings.offworkTime}
          onChange={(e) => handleSave({ offworkTime: e.target.value })}
          style={{ ...inputStyle, width: 'auto' }}
        />
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginTop: 4 }}>
          到达此时间（±15分钟）且停止工作后，触发深层心跳生成日报
        </div>
      </div>

      {/* ── 日报发送渠道 ── */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>日报发送渠道</div>
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 10 }}>配置后可一键发送到团队群组 · 留空不启用</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div>
            <label style={{ fontSize: 10, color: 'var(--color-text-tertiary)', display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
              <span style={{ width: 16, height: 16, borderRadius: 4, background: '#1DA1F2', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9, color: '#fff', fontWeight: 700 }}>钉</span>
              钉钉机器人 Webhook
            </label>
            <input type="text" placeholder="https://oapi.dingtalk.com/robot/send?access_token=..." value={settings.dingtalkWebhook}
              onChange={(e) => updateSettings({ dingtalkWebhook: e.target.value })}
              onBlur={(e) => handleSave({ dingtalkWebhook: e.target.value })}
              style={inputStyle}
              onFocus={(e) => { (e.target as HTMLInputElement).style.borderColor = '#1DA1F2'; }}
            />
          </div>
          <div>
            <label style={{ fontSize: 10, color: 'var(--color-text-tertiary)', display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
              <span style={{ width: 16, height: 16, borderRadius: 4, background: '#00C5A8', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9, color: '#fff', fontWeight: 700 }}>书</span>
              飞书机器人 Webhook
            </label>
            <input type="text" placeholder="https://open.feishu.cn/open-apis/bot/v2/hook/..." value={settings.feishuWebhook}
              onChange={(e) => updateSettings({ feishuWebhook: e.target.value })}
              onBlur={(e) => handleSave({ feishuWebhook: e.target.value })}
              style={inputStyle}
              onFocus={(e) => { (e.target as HTMLInputElement).style.borderColor = '#00C5A8'; }}
            />
          </div>
        </div>
      </div>

      {/* ── 系统 ── */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 8 }}>系统</div>

        {/* 数据目录 */}
        <div style={{ marginBottom: 10 }}>
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>数据目录</div>
          <div style={{ display: 'flex', gap: 6 }}>
            <input
              type="text"
              placeholder="留空使用默认目录"
              value={settings.dataDir}
              onChange={(e) => updateSettings({ dataDir: e.target.value })}
              onBlur={(e) => { handleSave({ dataDir: e.target.value }); (e.target as HTMLInputElement).style.borderColor = '#333'; }}
              style={{ ...inputStyle, flex: 1 }}
              onFocus={(e) => { (e.target as HTMLInputElement).style.borderColor = '#534AB7'; }}
            />
            <button
              onClick={async () => {
                try {
                  const selected = await open({ directory: true, multiple: false });
                  if (selected) {
                    handleSave({ dataDir: selected as string });
                  }
                } catch (e) {
                  console.error('[Settings] browse data dir:', e);
                }
              }}
              style={{ padding: '8px 12px', fontSize: 11, borderRadius: 6, cursor: 'pointer', background: 'var(--color-background-secondary)', border: '0.5px solid var(--color-border-primary)', color: 'var(--color-text-secondary)' }}
            >浏览</button>
          </div>
          <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginTop: 4 }}>
            设为空则使用默认目录 · 每日数据存放在 {settings.dataDir ? settings.dataDir + '/YYYY-MM-DD/' : '%APPDATA%'} 下
          </div>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '10px 12px', background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)' }}>
          <div>
            <div style={{ fontSize: 12, color: 'var(--color-text-primary)', marginBottom: 2 }}>开机自动启动</div>
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>登录后 Auto-Heart 自动运行，常驻托盘</div>
          </div>
          <button onClick={toggleAutostart} disabled={autostartLoading}
            style={{ width: 44, height: 24, borderRadius: 12, border: 'none', cursor: autostartLoading ? 'default' : 'pointer',
              background: autostartEnabled ? 'var(--color-brand)' : 'var(--color-border-primary)',
              position: 'relative', transition: 'background 0.2s', opacity: autostartLoading ? 0.6 : 1, flexShrink: 0 }}
          >
            <span style={{ position: 'absolute', top: 3, left: autostartEnabled ? 23 : 3,
              width: 18, height: 18, borderRadius: '50%', background: '#fff',
              transition: 'left 0.2s', boxShadow: '0 1px 3px rgba(0,0,0,0.3)' }} />
          </button>
        </div>
      </div>
    </div>
  );
}

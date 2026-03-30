import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { Settings, useSettings } from '../hooks/useSettings';

async function invokeWithTimeout<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error(`invoke "${cmd}" timeout after 5s`)), 5000),
  );
  return Promise.race([invoke<T>(cmd, args), timeout]) as Promise<T>;
}

interface ModelProvider {
  id: string;
  name: string;
  color: string;
  keyField: keyof Settings | null;
  models: string[];
  hint: string;
}

interface GitCommitEntry {
  repo_path: string;
  short_hash: string;
  summary: string;
  committed_at: string;
}

interface TrackingHealth {
  current_db_path: string;
  watch_paths: string[];
  repo_paths: string[];
  today_activity_snapshots: number;
  today_file_changes: number;
  today_operation_logs: number;
  today_git_commits: number;
  latest_activity_at: string | null;
  latest_file_change_at: string | null;
  latest_git_commit: GitCommitEntry | null;
}

const PROVIDERS: ModelProvider[] = [
  { id: 'kimi', name: 'Kimi', color: '#1A73E8', keyField: 'kimiApiKey', models: ['moonshot-v1-8k', 'moonshot-v1-32k', 'moonshot-v1-128k'], hint: '适合长上下文总结和通用办公。' },
  { id: 'qwen', name: 'Qwen', color: '#FF6200', keyField: 'qwenApiKey', models: ['qwen-turbo', 'qwen-plus', 'qwen-max', 'qwen-long'], hint: '响应快，适合综合日常场景。' },
  { id: 'minimax', name: 'MiniMax', color: '#6C5CE7', keyField: 'minimaxApiKey', models: [], hint: '助手型体验不错，适合对话。' },
  { id: 'gpt', name: 'GPT', color: '#10A37F', keyField: 'gptApiKey', models: ['gpt-4o-mini', 'gpt-4o', 'o1-mini', 'gpt-4-turbo'], hint: 'OpenAI 通用模型，稳定性好。' },
  { id: 'claude', name: 'Claude', color: '#E16B1A', keyField: 'claudeApiKey', models: ['claude-haiku-4-5', 'claude-sonnet-4-5', 'claude-opus-4-5'], hint: '文档理解和长文本表达更稳。' },
  { id: 'deepseek', name: 'DeepSeek', color: '#0066FF', keyField: 'deepseekApiKey', models: ['deepseek-chat', 'deepseek-reasoner'], hint: '代码和推理兼顾。' },
  { id: 'openrouter', name: 'OpenRouter', color: '#8B5CF6', keyField: 'openrouterApiKey', models: ['openai/gpt-4o-mini', 'anthropic/claude-sonnet-4-5', 'google/gemini-2.0-flash-001', 'deepseek/deepseek-chat'], hint: '方便切换多个供应商。' },
  { id: 'ollama', name: '本地', color: '#555555', keyField: null, models: [], hint: '离线运行，本地模型可用。' },
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

const cardStyle: React.CSSProperties = {
  background: 'var(--color-background-secondary)',
  borderRadius: 'var(--border-radius-lg)',
  border: '0.5px solid var(--color-border-tertiary)',
  padding: 14,
  display: 'flex',
  flexDirection: 'column',
  gap: 10,
};

const inputStyle: React.CSSProperties = {
  width: '100%',
  padding: '8px 10px',
  fontSize: 12,
  background: 'var(--color-background-tertiary)',
  border: '0.5px solid var(--color-border-primary)',
  borderRadius: 6,
  color: 'var(--color-text-primary)',
  boxSizing: 'border-box',
  outline: 'none',
};

function StatCard({ label, value, detail }: { label: string; value: string | number; detail?: string }) {
  return (
    <div
      style={{
        padding: '10px 12px',
        background: 'var(--color-background-tertiary)',
        borderRadius: 'var(--border-radius-md)',
        border: '0.5px solid var(--color-border-primary)',
      }}
    >
      <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>{label}</div>
      <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--color-text-primary)' }}>{value}</div>
      {detail ? <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginTop: 4 }}>{detail}</div> : null}
    </div>
  );
}

function LayerModelPicker({
  label,
  hint,
  selectedProvider,
  selectedModelName,
  settings,
  onProviderChange,
  onModelNameChange,
  onKeyChange,
}: {
  label: string;
  hint: string;
  selectedProvider: string;
  selectedModelName: string;
  settings: Settings;
  onProviderChange: (provider: string) => void;
  onModelNameChange: (name: string) => void;
  onKeyChange: (keyField: keyof Settings, value: string) => void;
}) {
  const provider = PROVIDERS.find((item) => item.id === selectedProvider) ?? PROVIDERS[0];
  const keyField = provider.keyField;
  const currentKey = keyField ? (settings[keyField] as string) : '';
  const isConfigured = provider.id === 'ollama' || !!currentKey;

  return (
    <section style={cardStyle}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>{label}</div>
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>{hint}</div>
        {isConfigured ? (
          <div
            style={{
              marginLeft: 'auto',
              padding: '2px 8px',
              borderRadius: 999,
              fontSize: 10,
              color: 'var(--color-text-success)',
              background: 'rgba(76,175,80,0.12)',
            }}
          >
            已配置
          </div>
        ) : null}
      </div>

      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
        {PROVIDERS.map((item) => {
          const configured = item.id === 'ollama' || !!(item.keyField ? settings[item.keyField] : 'ok');
          const selected = item.id === selectedProvider;
          return (
            <button
              key={item.id}
              onClick={() => onProviderChange(item.id)}
              style={{
                position: 'relative',
                padding: '6px 10px',
                borderRadius: 8,
                border: selected ? `1px solid ${item.color}` : '0.5px solid var(--color-border-primary)',
                background: selected ? `${item.color}18` : 'transparent',
                color: selected ? item.color : 'var(--color-text-secondary)',
                fontSize: 11,
                cursor: 'pointer',
              }}
            >
              {item.name}
              {configured ? (
                <span
                  style={{
                    position: 'absolute',
                    top: -3,
                    right: -3,
                    width: 7,
                    height: 7,
                    borderRadius: '50%',
                    background: 'var(--color-text-success)',
                    border: '1px solid var(--color-background-secondary)',
                  }}
                />
              ) : null}
            </button>
          );
        })}
      </div>

      <div
        style={{
          background: 'var(--color-background-tertiary)',
          borderRadius: 10,
          padding: 12,
          display: 'flex',
          flexDirection: 'column',
          gap: 8,
        }}
      >
        <div style={{ fontSize: 10, color: provider.color, fontWeight: 600 }}>{provider.hint}</div>

        {keyField ? (
          <div>
            <label style={{ display: 'block', fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
              {KEY_LABELS[keyField] ?? 'API Key'}
            </label>
            <input
              type="password"
              placeholder={KEY_PLACEHOLDERS[keyField] ?? 'sk-...'}
              value={currentKey}
              onChange={(event) => onKeyChange(keyField, event.target.value)}
              onBlur={(event) => onKeyChange(keyField, event.target.value)}
              style={inputStyle}
            />
          </div>
        ) : (
          <div>
            <label style={{ display: 'block', fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
              Ollama 服务地址
            </label>
            <input
              type="text"
              placeholder="http://localhost:11434"
              value={settings.ollamaBaseUrl}
              onChange={(event) => onKeyChange('ollamaBaseUrl', event.target.value)}
              onBlur={(event) => onKeyChange('ollamaBaseUrl', event.target.value)}
              style={inputStyle}
            />
          </div>
        )}

        <div>
          <label style={{ display: 'block', fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
            模型名称
          </label>
          {provider.models.length > 0 ? (
            <select
              value={selectedModelName || provider.models[0]}
              onChange={(event) => onModelNameChange(event.target.value)}
              style={{ ...inputStyle, cursor: 'pointer' }}
            >
              {provider.models.map((model) => (
                <option key={model} value={model}>
                  {model}
                </option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              placeholder={provider.id === 'ollama' ? 'qwen2.5:7b' : '输入实际模型名'}
              value={selectedModelName}
              onChange={(event) => onModelNameChange(event.target.value)}
              onBlur={(event) => onModelNameChange(event.target.value)}
              style={inputStyle}
            />
          )}
        </div>
      </div>
    </section>
  );
}

export default function SettingsTab() {
  const { settings, updateSettings, saveToHome, loading } = useSettings();
  const [watchPathInput, setWatchPathInput] = useState('');
  const [saved, setSaved] = useState(false);
  const [savedHome, setSavedHome] = useState(false);
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);
  const [trackingHealth, setTrackingHealth] = useState<TrackingHealth | null>(null);
  const [trackingHealthLoading, setTrackingHealthLoading] = useState(false);

  const loadTrackingHealth = useCallback(async () => {
    setTrackingHealthLoading(true);
    try {
      const nextHealth = await invokeWithTimeout<TrackingHealth>('get_tracking_health');
      setTrackingHealth(nextHealth);
    } catch (error) {
      console.error('[Settings] load tracking health failed:', error);
      setTrackingHealth(null);
    } finally {
      setTrackingHealthLoading(false);
    }
  }, []);

  useEffect(() => {
    invoke<boolean>('plugin:autostart|is_enabled')
      .then((value) => setAutostartEnabled(value))
      .catch(() => {});
    loadTrackingHealth();
  }, [loadTrackingHealth]);

  const handleSave = useCallback(
    async (patch: Partial<Settings>) => {
      await updateSettings(patch);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      setTimeout(() => {
        loadTrackingHealth();
      }, 150);
    },
    [loadTrackingHealth, updateSettings],
  );

  const handleSaveToHome = async () => {
    try {
      await saveToHome();
      setSavedHome(true);
      setTimeout(() => setSavedHome(false), 2000);
      loadTrackingHealth();
    } catch {
      // noop
    }
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
    } catch (error) {
      console.error(error);
    } finally {
      setAutostartLoading(false);
    }
  };

  const addWatchPath = () => {
    const trimmed = watchPathInput.trim();
    if (!trimmed || settings.watchPaths.includes(trimmed)) {
      return;
    }
    handleSave({ watchPaths: [...settings.watchPaths, trimmed] });
    setWatchPathInput('');
  };

  const removeWatchPath = (path: string) => {
    handleSave({ watchPaths: settings.watchPaths.filter((item) => item !== path) });
  };

  if (loading) {
    return <div style={{ padding: 16, fontSize: 12, color: 'var(--color-text-tertiary)' }}>加载设置中...</div>;
  }

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 18 }}>
      {saved ? (
        <div
          style={{
            fontSize: 11,
            color: 'var(--color-text-success)',
            background: 'rgba(76,175,80,0.1)',
            padding: '6px 10px',
            borderRadius: 8,
            textAlign: 'center',
          }}
        >
          设置已保存
        </div>
      ) : null}

      <section style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
          <div>
            <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>监听健康检查</div>
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginTop: 2 }}>
              用来确认今天是否真的抓到了前台活动、文件改动和 Git 提交。
            </div>
          </div>
          <button
            onClick={loadTrackingHealth}
            disabled={trackingHealthLoading}
            style={{
              padding: '6px 10px',
              fontSize: 11,
              borderRadius: 8,
              cursor: trackingHealthLoading ? 'default' : 'pointer',
              border: '0.5px solid var(--color-border-primary)',
              background: 'transparent',
              color: 'var(--color-text-secondary)',
              opacity: trackingHealthLoading ? 0.6 : 1,
            }}
          >
            {trackingHealthLoading ? '刷新中...' : '刷新'}
          </button>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, minmax(0, 1fr))', gap: 8 }}>
          <StatCard label="前台快照" value={trackingHealth?.today_activity_snapshots ?? 0} />
          <StatCard label="文件变更" value={trackingHealth?.today_file_changes ?? 0} />
          <StatCard label="操作日志" value={trackingHealth?.today_operation_logs ?? 0} />
          <StatCard label="Git 提交" value={trackingHealth?.today_git_commits ?? 0} />
        </div>

        <div
          style={{
            padding: '10px 12px',
            background: 'var(--color-background-tertiary)',
            borderRadius: 'var(--border-radius-md)',
            border: '0.5px solid var(--color-border-primary)',
          }}
        >
          <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>当前数据库</div>
          <div style={{ fontSize: 11, color: 'var(--color-text-primary)', wordBreak: 'break-all' }}>
            {trackingHealth?.current_db_path || '暂无'}
          </div>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, minmax(0, 1fr))', gap: 8 }}>
          <div
            style={{
              padding: '10px 12px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
              border: '0.5px solid var(--color-border-primary)',
            }}
          >
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>监听目录</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              {(trackingHealth?.watch_paths ?? []).slice(0, 6).map((path) => (
                <div key={path} style={{ fontSize: 11, color: 'var(--color-text-secondary)', wordBreak: 'break-all' }}>
                  {path}
                </div>
              ))}
              {(trackingHealth?.watch_paths ?? []).length === 0 ? (
                <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>当前没有监听目录</div>
              ) : null}
            </div>
          </div>

          <div
            style={{
              padding: '10px 12px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
              border: '0.5px solid var(--color-border-primary)',
            }}
          >
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>识别到的 Git 仓库</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              {(trackingHealth?.repo_paths ?? []).slice(0, 6).map((path) => (
                <div key={path} style={{ fontSize: 11, color: 'var(--color-text-secondary)', wordBreak: 'break-all' }}>
                  {path}
                </div>
              ))}
              {(trackingHealth?.repo_paths ?? []).length === 0 ? (
                <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>监听目录下还没有发现 Git 仓库</div>
              ) : null}
            </div>
          </div>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, minmax(0, 1fr))', gap: 8 }}>
          <StatCard label="最近活动快照" value={trackingHealth?.latest_activity_at?.slice(11, 16) ?? '暂无'} />
          <StatCard label="最近文件变更" value={trackingHealth?.latest_file_change_at?.slice(11, 16) ?? '暂无'} />
          <StatCard
            label="最近 Git 提交"
            value={trackingHealth?.latest_git_commit ? trackingHealth.latest_git_commit.short_hash : '暂无'}
            detail={trackingHealth?.latest_git_commit?.summary}
          />
        </div>
      </section>

      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>模型配置会同时作用于中层、深层和对话。</div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {savedHome ? <span style={{ fontSize: 10, color: 'var(--color-text-success)' }}>已保存到本地配置</span> : null}
          <button
            onClick={handleSaveToHome}
            style={{
              padding: '6px 12px',
              fontSize: 11,
              borderRadius: 8,
              cursor: 'pointer',
              background: 'var(--color-brand)',
              border: 'none',
              color: '#fff',
            }}
          >
            保存到本地
          </button>
        </div>
      </div>

      <LayerModelPicker
        label="统一模型"
        hint="中层心跳、深层心跳、对话共用"
        selectedProvider={settings.middleModel}
        selectedModelName={settings.middleModelName}
        settings={settings}
        onProviderChange={(provider) =>
          handleSave({
            middleModel: provider,
            middleModelName: '',
            deepModel: provider,
            deepModelName: '',
            chatModel: provider,
            chatModelName: '',
          })
        }
        onModelNameChange={(name) =>
          handleSave({
            middleModelName: name,
            deepModelName: name,
            chatModelName: name,
          })
        }
        onKeyChange={(field, value) => handleSave({ [field]: value } as Partial<Settings>)}
      />

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>意图文档路径</div>
        <input
          type="text"
          placeholder="支持 Notion / Obsidian / 本地 txt 路径"
          value={settings.intentDocPath}
          onChange={(event) => updateSettings({ intentDocPath: event.target.value })}
          onBlur={(event) => handleSave({ intentDocPath: event.target.value })}
          style={inputStyle}
        />
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
          这个文档会作为工作计划的补充来源，帮助日报理解上下文。
        </div>
      </section>

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>感知范围</div>
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
          建议把常用项目目录都加进来，这样文件监听和 Git 汇总才会更完整。
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <input
            type="text"
            placeholder="添加项目路径，比如 D:/company 或 D:/Agent"
            value={watchPathInput}
            onChange={(event) => setWatchPathInput(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && addWatchPath()}
            style={{ ...inputStyle, flex: 1 }}
          />
          <button
            onClick={addWatchPath}
            style={{
              padding: '8px 14px',
              fontSize: 12,
              borderRadius: 8,
              cursor: 'pointer',
              background: 'var(--color-brand-light)',
              border: '0.5px solid var(--color-brand-ring)',
              color: 'var(--color-brand)',
            }}
          >
            添加
          </button>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {settings.watchPaths.length === 0 ? (
            <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>当前未手动添加目录，将使用系统自动探测目录。</div>
          ) : (
            settings.watchPaths.map((path) => (
              <div
                key={path}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  padding: '8px 10px',
                  background: 'var(--color-background-tertiary)',
                  borderRadius: 'var(--border-radius-md)',
                  border: '0.5px solid var(--color-border-primary)',
                }}
              >
                <span style={{ flex: 1, fontSize: 11, color: 'var(--color-text-secondary)', wordBreak: 'break-all' }}>{path}</span>
                <button
                  onClick={() => removeWatchPath(path)}
                  style={{
                    border: 'none',
                    background: 'transparent',
                    color: 'var(--color-text-tertiary)',
                    cursor: 'pointer',
                    fontSize: 11,
                  }}
                >
                  删除
                </button>
              </div>
            ))
          )}
        </div>
      </section>

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>静默模式</div>
        <div style={{ display: 'flex', gap: 8 }}>
          {([
            { id: 'focus' as const, label: '专注', desc: '只在高优先级场景提醒。' },
            { id: 'normal' as const, label: '正常', desc: '平衡提醒和干扰感。' },
            { id: 'open' as const, label: '开放', desc: '观察和建议都会展示。' },
          ]).map((mode) => (
            <button
              key={mode.id}
              onClick={() => handleSave({ silenceMode: mode.id })}
              style={{
                flex: 1,
                padding: '10px 12px',
                textAlign: 'left',
                borderRadius: 'var(--border-radius-md)',
                cursor: 'pointer',
                border: settings.silenceMode === mode.id ? '0.5px solid var(--color-brand)' : '0.5px solid var(--color-border-primary)',
                background: settings.silenceMode === mode.id ? 'var(--color-brand-light)' : 'transparent',
                color: settings.silenceMode === mode.id ? 'var(--color-brand)' : 'var(--color-text-secondary)',
              }}
            >
              <div style={{ fontSize: 12, fontWeight: 600 }}>{mode.label}</div>
              <div style={{ fontSize: 10, opacity: 0.8, marginTop: 4 }}>{mode.desc}</div>
            </button>
          ))}
        </div>
      </section>

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>下班时间</div>
        <input
          type="time"
          value={settings.offworkTime}
          onChange={(event) => handleSave({ offworkTime: event.target.value })}
          style={{ ...inputStyle, width: 140 }}
        />
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
          到达这个时间并满足离岗条件后，更适合触发自动日报整理。
        </div>
      </section>

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>日报发送渠道</div>
        <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
          配置后可以一键把日报发到钉钉或飞书。留空则不启用。
        </div>
        <div>
          <label style={{ display: 'block', fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
            钉钉 Webhook
          </label>
          <input
            type="text"
            placeholder="https://oapi.dingtalk.com/robot/send?access_token=..."
            value={settings.dingtalkWebhook}
            onChange={(event) => updateSettings({ dingtalkWebhook: event.target.value })}
            onBlur={(event) => handleSave({ dingtalkWebhook: event.target.value })}
            style={inputStyle}
          />
        </div>
        <div>
          <label style={{ display: 'block', fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
            飞书 Webhook
          </label>
          <input
            type="text"
            placeholder="https://open.feishu.cn/open-apis/bot/v2/hook/..."
            value={settings.feishuWebhook}
            onChange={(event) => updateSettings({ feishuWebhook: event.target.value })}
            onBlur={(event) => handleSave({ feishuWebhook: event.target.value })}
            style={inputStyle}
          />
        </div>
      </section>

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>系统</div>

        <div>
          <label style={{ display: 'block', fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
            数据目录
          </label>
          <div style={{ display: 'flex', gap: 8 }}>
            <input
              type="text"
              placeholder="留空则使用默认目录"
              value={settings.dataDir}
              onChange={(event) => updateSettings({ dataDir: event.target.value })}
              onBlur={(event) => handleSave({ dataDir: event.target.value })}
              style={{ ...inputStyle, flex: 1 }}
            />
            <button
              onClick={async () => {
                try {
                  const selected = await open({ directory: true, multiple: false });
                  if (selected) {
                    handleSave({ dataDir: selected as string });
                  }
                } catch (error) {
                  console.error('[Settings] browse data dir failed:', error);
                }
              }}
              style={{
                padding: '8px 12px',
                fontSize: 11,
                borderRadius: 8,
                cursor: 'pointer',
                background: 'var(--color-background-tertiary)',
                border: '0.5px solid var(--color-border-primary)',
                color: 'var(--color-text-secondary)',
              }}
            >
              浏览
            </button>
          </div>
          <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginTop: 4 }}>
            每日数据库会保存在 {settings.dataDir ? `${settings.dataDir}/YYYY-MM-DD/` : '应用默认目录/YYYY-MM-DD/'}。
          </div>
        </div>

        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '10px 12px',
            background: 'var(--color-background-tertiary)',
            borderRadius: 'var(--border-radius-md)',
            border: '0.5px solid var(--color-border-primary)',
          }}
        >
          <div>
            <div style={{ fontSize: 12, color: 'var(--color-text-primary)', marginBottom: 2 }}>开机自动启动</div>
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>登录后自动运行并继续监听。</div>
          </div>
          <button
            onClick={toggleAutostart}
            disabled={autostartLoading}
            style={{
              width: 44,
              height: 24,
              borderRadius: 12,
              border: 'none',
              cursor: autostartLoading ? 'default' : 'pointer',
              background: autostartEnabled ? 'var(--color-brand)' : 'var(--color-border-primary)',
              position: 'relative',
              opacity: autostartLoading ? 0.6 : 1,
            }}
          >
            <span
              style={{
                position: 'absolute',
                top: 3,
                left: autostartEnabled ? 23 : 3,
                width: 18,
                height: 18,
                borderRadius: '50%',
                background: '#fff',
                transition: 'left 0.2s',
                boxShadow: '0 1px 3px rgba(0,0,0,0.3)',
              }}
            />
          </button>
        </div>
      </section>
    </div>
  );
}

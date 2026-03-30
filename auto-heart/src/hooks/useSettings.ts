import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** 带 5 秒超时的 invoke，防止 Rust 端阻塞导致 UI 永久挂起 */
async function invokeWithTimeout<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error(`invoke "${cmd}" timeout after 5s`)), 5000)
  );
  return Promise.race([invoke<T>(cmd, args), timeout]) as Promise<T>;
}

export interface Settings {
  // 基础
  dataDir: string;          // 自定义数据目录，空=默认
  intentDocPath: string;
  watchPaths: string[];
  silenceMode: 'focus' | 'normal' | 'open';
  offworkTime: string;
  // 通知渠道
  dingtalkWebhook: string;
  feishuWebhook: string;
  // 模型选择
  middleModel: string;
  middleModelName: string;
  deepModel: string;
  deepModelName: string;
  chatModel: string;
  chatModelName: string;
  // API Keys
  kimiApiKey: string;
  claudeApiKey: string;
  gptApiKey: string;
  qwenApiKey: string;
  minimaxApiKey: string;
  deepseekApiKey: string;
  openrouterApiKey: string;
  // Ollama
  ollamaBaseUrl: string;
  // 主动建议
  proactiveSuggestions: boolean;
  criticalKeywords: string;
  themeMode: 'system' | 'light' | 'dark';
}

const DEFAULT_SETTINGS: Settings = {
  dataDir: '',
  intentDocPath: '',
  watchPaths: [],
  silenceMode: 'normal',
  offworkTime: '18:00',
  dingtalkWebhook: '',
  feishuWebhook: '',
  middleModel: 'kimi',
  middleModelName: '',
  deepModel: 'claude',
  deepModelName: '',
  chatModel: 'kimi',
  chatModelName: '',
  kimiApiKey: '',
  claudeApiKey: '',
  gptApiKey: '',
  qwenApiKey: '',
  minimaxApiKey: '',
  deepseekApiKey: '',
  openrouterApiKey: '',
  ollamaBaseUrl: '',
  proactiveSuggestions: true,
  criticalKeywords: 'auth,security,password,token,payment,config,middleware,permission',
  themeMode: 'system',
};

function fromRust(raw: Record<string, unknown>): Settings {
  return {
    dataDir: (raw.data_dir as string) ?? '',
    intentDocPath: (raw.intent_doc_path as string) ?? '',
    watchPaths: (raw.watch_paths as string[]) ?? [],
    silenceMode: ((raw.silence_mode as string) ?? 'normal') as Settings['silenceMode'],
    offworkTime: (raw.offwork_time as string) ?? '18:00',
    dingtalkWebhook: (raw.dingtalk_webhook as string) ?? '',
    feishuWebhook: (raw.feishu_webhook as string) ?? '',
    middleModel: (raw.middle_model as string) ?? 'kimi',
    middleModelName: (raw.middle_model_name as string) ?? '',
    deepModel: (raw.deep_model as string) ?? 'claude',
    deepModelName: (raw.deep_model_name as string) ?? '',
    chatModel: ((raw.chat_model as string) || (raw.middle_model as string)) ?? 'kimi',
    chatModelName: ((raw.chat_model_name as string) || (raw.middle_model_name as string)) ?? '',
    kimiApiKey: (raw.kimi_api_key as string) ?? '',
    claudeApiKey: (raw.claude_api_key as string) ?? '',
    gptApiKey: (raw.gpt_api_key as string) ?? '',
    qwenApiKey: (raw.qwen_api_key as string) ?? '',
    minimaxApiKey: (raw.minimax_api_key as string) ?? '',
    deepseekApiKey: (raw.deepseek_api_key as string) ?? '',
    openrouterApiKey: (raw.openrouter_api_key as string) ?? '',
    ollamaBaseUrl: (raw.ollama_base_url as string) ?? '',
    proactiveSuggestions: (raw.proactive_suggestions as boolean) ?? true,
    criticalKeywords: ((raw.critical_keywords as string) || 'auth,security,password,token,payment,config,middleware,permission') ?? 'auth,security,password,token,payment,config,middleware,permission',
    themeMode: ((raw.theme_mode as string) ?? 'system') as Settings['themeMode'],
  };
}

function toRust(s: Settings): Record<string, unknown> {
  return {
    data_dir: s.dataDir,
    intent_doc_path: s.intentDocPath,
    watch_paths: s.watchPaths,
    silence_mode: s.silenceMode,
    offwork_time: s.offworkTime,
    dingtalk_webhook: s.dingtalkWebhook,
    feishu_webhook: s.feishuWebhook,
    middle_model: s.middleModel,
    middle_model_name: s.middleModelName,
    deep_model: s.deepModel,
    deep_model_name: s.deepModelName,
    chat_model: s.chatModel,
    chat_model_name: s.chatModelName,
    kimi_api_key: s.kimiApiKey,
    claude_api_key: s.claudeApiKey,
    gpt_api_key: s.gptApiKey,
    qwen_api_key: s.qwenApiKey,
    minimax_api_key: s.minimaxApiKey,
    deepseek_api_key: s.deepseekApiKey,
    openrouter_api_key: s.openrouterApiKey,
    ollama_base_url: s.ollamaBaseUrl,
    proactive_suggestions: s.proactiveSuggestions,
    critical_keywords: s.criticalKeywords,
    theme_mode: s.themeMode,
  };
}

export function useSettings() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invokeWithTimeout<Record<string, unknown>>('load_settings_cmd')
      .then((raw) => setSettings(fromRust(raw)))
      .catch((e) => console.warn('[useSettings] load timeout:', e))
      .finally(() => setLoading(false));
  }, []);

  const updateSettings = useCallback(async (patch: Partial<Settings>) => {
    const updated = { ...settings, ...patch };
    setSettings(updated);
    try {
      await invokeWithTimeout('save_settings', { newSettings: toRust(updated) });
    } catch (err) {
      console.error('[useSettings] save failed:', err);
    }
  }, [settings]);

  /// 保存到用户主目录 ~/.autoheart
  const saveToHome = useCallback(async () => {
    try {
      await invokeWithTimeout('save_settings_to_home', { newSettings: toRust(settings) });
    } catch (err) {
      console.error('[useSettings] saveToHome failed:', err);
      throw err;
    }
  }, [settings]);

  return { settings, updateSettings, saveToHome, loading };
}

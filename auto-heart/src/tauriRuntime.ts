/**
 * 是否为 Tauri WebView 环境（纯浏览器打开 localhost 时没有 __TAURI_INTERNALS__）。
 */
export function isTauriRuntime(): boolean {
  if (typeof window === 'undefined') return false;
  const w = window as unknown as {
    __TAURI_INTERNALS__?: { metadata?: unknown };
  };
  return w.__TAURI_INTERNALS__?.metadata != null;
}

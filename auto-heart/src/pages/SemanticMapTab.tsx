import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** 带 5 秒超时的 invoke，防止 Rust 端阻塞导致 UI 永久挂起 */
async function invokeWithTimeout<T>(cmd: string): Promise<T> {
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error(`invoke "${cmd}" timeout after 5s`)), 5000)
  );
  return Promise.race([invoke<T>(cmd), timeout]) as Promise<T>;
}

interface SemanticModule {
  id: string;
  module_name: string;
  description: string;
  understanding: string;
  updated_at: string;
}

interface DecisionEntry {
  id: string;
  description: string;
  reason: string;
  related_file: string;
  created_at: string;
}

interface TechDebtEntry {
  id: string;
  description: string;
  impact: string;
  introduced_at: string;
}


function formatRelativeTime(dateStr: string): string {
  try {
    const d = new Date(dateStr.replace(' ', 'T') + 'Z');
    const diff = Date.now() - d.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 60) return `${mins} 分钟前`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours} 小时前`;
    return `${Math.floor(hours / 24)} 天前`;
  } catch {
    return dateStr;
  }
}

export default function SemanticMapTab() {
  const [modules, setModules] = useState<SemanticModule[]>([]);
  const [decisions, setDecisions] = useState<DecisionEntry[]>([]);
  const [debts, setDebts] = useState<TechDebtEntry[]>([]);

  const loadData = async () => {
    try {
      const [mods, decs, tds] = await Promise.all([
        invokeWithTimeout<SemanticModule[]>('get_semantic_modules'),
        invokeWithTimeout<DecisionEntry[]>('get_decision_log'),
        invokeWithTimeout<TechDebtEntry[]>('get_tech_debt'),
      ]);
      setModules(mods);
      setDecisions(decs);
      setDebts(tds);
    } catch {
      // 网络或数据库错误，保持空状态
    }
  };

  useEffect(() => {
    loadData();
    // 中层心跳触发时刷新
    const unlisten = listen('heartbeat:middle', () => loadData());
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 16 }}>

      {/* 项目理解摘要 */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6, display: 'flex', alignItems: 'center', gap: 6 }}>
          项目理解摘要 · {modules.length} 个模块
          </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {modules.length === 0 ? (
            <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', textAlign: 'center', padding: '20px', background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)' }}>
              暂无模块理解 · 配置模型后可开始分析
            </div>
          ) : modules.map((m) => (
            <div key={m.id} style={{ background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-lg)', padding: 12 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                <span style={{ fontSize: 13, fontWeight: 500, color: 'var(--color-brand)' }}>{m.module_name}</span>
                <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
                  {formatRelativeTime(m.updated_at)}
                </span>
              </div>
              <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>{m.description}</div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', lineHeight: 1.5 }}>{m.understanding}</div>
            </div>
          ))}
        </div>
      </div>

      {/* 决策日志 */}
      {decisions.length > 0 && (
        <div>
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>最近决策日志</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {decisions.map((d) => (
              <div key={d.id} style={{ background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)', padding: '8px 10px', borderLeft: '2px solid var(--color-brand)' }}>
                <div style={{ fontSize: 12, color: 'var(--color-text-primary)', marginBottom: 3 }}>{d.description}</div>
                <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
                  {formatRelativeTime(d.created_at)} · {d.related_file}
                </div>
                {d.reason && (
                  <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginTop: 3 }}>原因：{d.reason}</div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 技术债清单 */}
      {debts.length > 0 && (
        <div>
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>技术债务 · {debts.length} 项</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
            {debts.map((td) => (
              <div key={td.id} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '7px 10px', background: 'var(--color-background-warning)', borderRadius: 'var(--border-radius-md)' }}>
                <div style={{ width: 5, height: 5, borderRadius: '50%', background: 'var(--color-text-warning)', flexShrink: 0 }} />
                <div style={{ flex: 1 }}>
                  <div style={{ fontSize: 12, color: 'var(--color-text-warning)' }}>{td.description}</div>
                  <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginTop: 2 }}>
                    引入于 {formatRelativeTime(td.introduced_at)} · {td.impact}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

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

// Demo 数据
const DEMO_MODULES: SemanticModule[] = [
  { id: '1', module_name: 'auth/guard.ts', description: '权限守卫模块', understanding: '负责验证用户身份和访问权限，入口为 verify() 函数', updated_at: '2小时前' },
  { id: '2', module_name: 'UserService', description: '用户服务', understanding: '处理用户 CRUD 操作，当前缺少 batch 接口', updated_at: '昨天' },
];

const DEMO_DECISIONS: DecisionEntry[] = [
  { id: '1', description: '从 middleware 迁移到 Guard 模式处理权限校验', reason: '提升可维护性，便于单元测试', related_file: 'auth/guard.ts', created_at: '今天 10:30' },
  { id: '2', description: '选择 REST 而非 GraphQL 实现 dashboard 接口', reason: '与现有基础设施一致，减少学习成本', related_file: 'dashboard/api.ts', created_at: '昨天 15:00' },
];

const DEMO_DEBTS: TechDebtEntry[] = [
  { id: '1', description: 'verify() 缺少 refreshToken 过期校验', impact: '安全风险 · auth/guard.ts', introduced_at: '3天前' },
  { id: '2', description: 'UserService 未实现 batch 接口', impact: '性能瓶颈 · 批量操作场景', introduced_at: '上周' },
];

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
  const [isDemo, setIsDemo] = useState(false);

  const loadData = async () => {
    try {
      const [mods, decs, tds] = await Promise.all([
        invoke<SemanticModule[]>('get_semantic_modules'),
        invoke<DecisionEntry[]>('get_decision_log'),
        invoke<TechDebtEntry[]>('get_tech_debt'),
      ]);
      if (mods.length === 0 && decs.length === 0) {
        setModules(DEMO_MODULES);
        setDecisions(DEMO_DECISIONS);
        setDebts(DEMO_DEBTS);
        setIsDemo(true);
      } else {
        setModules(mods);
        setDecisions(decs);
        setDebts(tds);
        setIsDemo(false);
      }
    } catch {
      setModules(DEMO_MODULES);
      setDecisions(DEMO_DECISIONS);
      setDebts(DEMO_DEBTS);
      setIsDemo(true);
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
          {isDemo && <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)', background: 'var(--color-background-secondary)', padding: '1px 6px', borderRadius: 4 }}>演示</span>}
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {modules.map((m) => (
            <div key={m.id} style={{ background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-lg)', padding: 12 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                <span style={{ fontSize: 13, fontWeight: 500, color: 'var(--color-brand)' }}>{m.module_name}</span>
                <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
                  {isDemo ? m.updated_at : formatRelativeTime(m.updated_at)}
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
                  {isDemo ? d.created_at : formatRelativeTime(d.created_at)} · {d.related_file}
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
                    引入于 {isDemo ? td.introduced_at : formatRelativeTime(td.introduced_at)} · {td.impact}
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

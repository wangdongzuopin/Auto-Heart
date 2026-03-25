import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useMessageQueue } from '../hooks/useMessageQueue';
import { useSettings } from '../hooks/useSettings';

interface TodayTask {
  time: string;
  task: string;
  tag: string;
  status: 'pending' | 'active' | 'done';
}

interface ReportData {
  id: string;
  date: string;
  content: string;
  status: 'draft' | 'confirmed' | 'sent';
}

const DEMO_TASKS: TodayTask[] = [
  { time: '10:00', task: 'refreshToken 过期校验', tag: 'auth/guard.ts', status: 'pending' },
  { time: '13:00', task: 'dashboard 接口联调', tag: 'UserService', status: 'pending' },
  { time: '14:30', task: '技术评审 — 我来帮你整理', tag: '进行中', status: 'active' },
  { time: '18:00', task: '日报', tag: '我来写', status: 'pending' },
];

export default function TodayTab() {
  const [tasks, setTasks] = useState<TodayTask[]>([]);
  const [intent, setIntent] = useState<{ raw_text: string; parsed: boolean } | null>(null);
  const [report, setReport] = useState<ReportData | null>(null);
  const [isDemo, setIsDemo] = useState(false);
  const [editingReport, setEditingReport] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [sending, setSending] = useState<string | null>(null); // 'dingtalk' | 'feishu'
  const [sendResult, setSendResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const { messages, dismiss, ack } = useMessageQueue();
  const { settings } = useSettings();

  const loadTasks = useCallback(async () => {
    try {
      const list = await invoke<TodayTask[]>('get_today_tasks');
      if (list.length === 0) { setTasks(DEMO_TASKS); setIsDemo(true); }
      else { setTasks(list); setIsDemo(false); }
    } catch { setTasks(DEMO_TASKS); setIsDemo(true); }
  }, []);

  const loadIntent = useCallback(async () => {
    try {
      const rec = await invoke<{ raw_text: string; parsed: boolean } | null>('get_today_intent');
      setIntent(rec);
    } catch { setIntent(null); }
  }, []);

  const loadReport = useCallback(async () => {
    try {
      const r = await invoke<ReportData | null>('get_today_report');
      setReport(r);
      if (r && !editingReport) setEditContent(r.content);
    } catch { setReport(null); }
  }, [editingReport]);

  useEffect(() => {
    loadTasks(); loadIntent(); loadReport();

    let unlistenParsed: (() => void) | undefined;
    let unlistenReport: (() => void) | undefined;
    let unlistenShallow: (() => void) | undefined;

    const setup = async () => {
      unlistenParsed = await listen('intent:parsed', () => { loadTasks(); loadIntent(); });
      unlistenReport = await listen('daily_report:ready', () => loadReport());
      unlistenShallow = await listen('heartbeat:shallow', () => loadIntent());
    };
    setup();
    return () => { unlistenParsed?.(); unlistenReport?.(); unlistenShallow?.(); };
  }, [loadTasks, loadIntent, loadReport]);

  const saveEdit = async () => {
    if (!report) return;
    try {
      await invoke('update_report_content', { date: report.date, content: editContent });
      setEditingReport(false);
      loadReport();
    } catch (e) { console.error(e); }
  };

  const sendReport = async (channel: 'dingtalk' | 'feishu') => {
    if (!report) return;
    setSending(channel);
    setSendResult(null);
    try {
      await invoke('send_daily_report', { date: report.date, channel });
      setSendResult({ ok: true, msg: channel === 'dingtalk' ? '已发送到钉钉 ✓' : '已发送到飞书 ✓' });
      loadReport();
    } catch (e) {
      setSendResult({ ok: false, msg: String(e) });
    } finally {
      setSending(null);
      setTimeout(() => setSendResult(null), 4000);
    }
  };

  const hasDingtalk = !!settings.dingtalkWebhook;
  const hasFeishu = !!settings.feishuWebhook;

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>

      {/* 今日意图原文 */}
      {intent && (
        <div>
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6, display: 'flex', alignItems: 'center', gap: 6 }}>
            今日意图
            <span style={{ fontSize: 9, padding: '1px 6px', borderRadius: 4, background: intent.parsed ? 'rgba(76,175,80,0.1)' : 'var(--color-brand-light)', color: intent.parsed ? 'var(--color-text-success)' : 'var(--color-brand)', border: intent.parsed ? 'none' : '0.5px solid var(--color-brand-ring)' }}>
              {intent.parsed ? '已解析' : '等待中层心跳解析...'}
            </span>
          </div>
          <div style={{ background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)', padding: '10px 12px', fontSize: 12, color: 'var(--color-text-secondary)', lineHeight: 1.8, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
            {intent.raw_text}
          </div>
        </div>
      )}

      {/* 任务列表 */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6, display: 'flex', alignItems: 'center', gap: 6 }}>
          {isDemo ? '任务示例' : '今日任务'}
          {isDemo && <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)', background: 'var(--color-background-secondary)', padding: '1px 6px', borderRadius: 4 }}>配置意图文档路径后自动替换</span>}
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
          {tasks.map((t, i) => (
            <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '7px 10px', background: t.status === 'active' ? 'var(--color-brand-light)' : t.status === 'done' ? 'transparent' : 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)', border: t.status === 'active' ? '0.5px solid var(--color-brand-ring)' : '0.5px solid transparent', opacity: t.status === 'done' ? 0.45 : 1, transition: 'all 0.15s' }}>
              <span style={{ fontSize: 11, fontWeight: 500, color: t.time ? 'var(--color-brand)' : 'var(--color-text-tertiary)', minWidth: 36 }}>{t.time || '--'}</span>
              <span style={{ fontSize: 12, color: t.status === 'active' ? 'var(--color-brand-dark)' : 'var(--color-text-primary)', flex: 1, textDecoration: t.status === 'done' ? 'line-through' : 'none' }}>{t.task}</span>
              {t.tag && <span style={{ fontSize: 10, color: t.status === 'active' ? 'var(--color-brand-muted)' : 'var(--color-text-tertiary)', background: 'var(--color-background-tertiary)', padding: '1px 6px', borderRadius: 4 }}>{t.tag}</span>}
            </div>
          ))}
        </div>
      </div>

      {/* 消息队列 */}
      <div>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6, display: 'flex', alignItems: 'center', gap: 6 }}>
          等待发送的消息
          {messages.length > 0 && <span style={{ fontSize: 9, background: 'var(--color-brand)', color: '#fff', padding: '1px 5px', borderRadius: 8 }}>{messages.length}</span>}
        </div>
        {messages.length === 0 ? (
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', padding: '8px 10px', background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)', textAlign: 'center' }}>暂无待发送消息</div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
            {messages.map((m) => (
              <div key={m.id} style={{ display: 'flex', alignItems: 'flex-start', gap: 8, padding: '8px 10px', background: m.priority === 0 ? 'rgba(255,80,80,0.08)' : m.priority === 1 ? 'var(--color-background-warning)' : 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-md)' }}>
                <div style={{ width: 5, height: 5, borderRadius: '50%', marginTop: 4, background: m.priority === 0 ? '#ff5050' : m.priority === 1 ? 'var(--color-text-warning)' : 'var(--color-border-secondary)', flexShrink: 0 }} />
                <div style={{ flex: 1 }}>
                  <div style={{ fontSize: 11, fontWeight: 500, color: m.priority <= 1 ? 'var(--color-text-warning)' : 'var(--color-text-secondary)', marginBottom: 2 }}>{m.title}</div>
                  <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', lineHeight: 1.4 }}>{m.content}</div>
                </div>
                <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
                  <span style={{ fontSize: 10, color: 'var(--color-brand)', background: 'var(--color-brand-light)', padding: '2px 6px', borderRadius: 4, cursor: 'pointer', border: '0.5px solid var(--color-brand-ring)' }} onClick={() => ack(m.id)}>帮我改</span>
                  <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)', background: 'var(--color-background-tertiary)', padding: '2px 6px', borderRadius: 4, cursor: 'pointer' }} onClick={() => dismiss(m.id)}>忽略</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 日报区块 */}
      {report && (
        <div style={{ background: 'var(--color-background-secondary)', borderRadius: 'var(--border-radius-lg)', padding: 14, border: report.status === 'sent' ? '0.5px solid rgba(76,175,80,0.3)' : '0.5px solid var(--color-brand-ring)' }}>
          {/* 日报标题栏 */}
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 10 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ fontSize: 12, fontWeight: 500, color: 'var(--color-text-primary)' }}>今日日报</span>
              <span style={{ fontSize: 9, padding: '1px 6px', borderRadius: 4, background: report.status === 'sent' ? 'rgba(76,175,80,0.15)' : report.status === 'confirmed' ? 'var(--color-brand-light)' : 'rgba(255,176,32,0.12)', color: report.status === 'sent' ? 'var(--color-text-success)' : report.status === 'confirmed' ? 'var(--color-brand)' : 'var(--color-text-warning)' }}>
                {report.status === 'sent' ? '已发送' : report.status === 'confirmed' ? '已确认' : '草稿'}
              </span>
            </div>
            {report.status !== 'sent' && !editingReport && (
              <span style={{ fontSize: 10, color: 'var(--color-brand)', cursor: 'pointer' }} onClick={() => { setEditingReport(true); setEditContent(report.content); }}>编辑</span>
            )}
          </div>

          {/* 日报内容 / 编辑器 */}
          {editingReport ? (
            <div>
              <textarea
                value={editContent}
                onChange={(e) => setEditContent(e.target.value)}
                rows={8}
                style={{ width: '100%', padding: '8px 10px', fontSize: 12, background: 'var(--color-background-tertiary)', border: '0.5px solid var(--color-brand-ring)', borderRadius: 6, color: 'var(--color-text-primary)', resize: 'vertical', outline: 'none', lineHeight: 1.6, boxSizing: 'border-box' }}
              />
              <div style={{ display: 'flex', gap: 6, marginTop: 8 }}>
                <button onClick={saveEdit} style={{ flex: 1, padding: '6px', fontSize: 11, borderRadius: 6, cursor: 'pointer', background: 'var(--color-brand)', border: 'none', color: '#fff' }}>保存修改</button>
                <button onClick={() => setEditingReport(false)} style={{ padding: '6px 12px', fontSize: 11, borderRadius: 6, cursor: 'pointer', background: 'transparent', border: '0.5px solid var(--color-border-primary)', color: 'var(--color-text-tertiary)' }}>取消</button>
              </div>
            </div>
          ) : (
            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', lineHeight: 1.7, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
              {report.content}
            </div>
          )}

          {/* 发送按钮组 */}
          {report.status !== 'sent' && !editingReport && (hasDingtalk || hasFeishu) && (
            <div style={{ marginTop: 12, display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {hasDingtalk && (
                <button
                  onClick={() => sendReport('dingtalk')}
                  disabled={!!sending}
                  style={{ padding: '6px 12px', fontSize: 11, borderRadius: 6, cursor: sending ? 'default' : 'pointer', background: sending === 'dingtalk' ? 'var(--color-background-tertiary)' : 'var(--color-brand-light)', border: '0.5px solid var(--color-brand-ring)', color: 'var(--color-brand)', opacity: sending && sending !== 'dingtalk' ? 0.5 : 1, transition: 'all 0.15s' }}
                >
                  {sending === 'dingtalk' ? '发送中...' : '发到钉钉'}
                </button>
              )}
              {hasFeishu && (
                <button
                  onClick={() => sendReport('feishu')}
                  disabled={!!sending}
                  style={{ padding: '6px 12px', fontSize: 11, borderRadius: 6, cursor: sending ? 'default' : 'pointer', background: sending === 'feishu' ? 'var(--color-background-tertiary)' : 'var(--color-brand-light)', border: '0.5px solid var(--color-brand-ring)', color: 'var(--color-brand)', opacity: sending && sending !== 'feishu' ? 0.5 : 1, transition: 'all 0.15s' }}
                >
                  {sending === 'feishu' ? '发送中...' : '发到飞书'}
                </button>
              )}
            </div>
          )}

          {/* 发送结果提示 */}
          {sendResult && (
            <div style={{ marginTop: 8, fontSize: 11, color: sendResult.ok ? 'var(--color-text-success)' : '#ff5050', padding: '4px 8px', background: sendResult.ok ? 'rgba(76,175,80,0.1)' : 'rgba(255,80,80,0.08)', borderRadius: 4 }}>
              {sendResult.msg}
            </div>
          )}

          {/* 未配置 Webhook 提示 */}
          {report.status !== 'sent' && !hasDingtalk && !hasFeishu && (
            <div style={{ marginTop: 10, fontSize: 10, color: 'var(--color-text-tertiary)', textAlign: 'center' }}>
              在「设置」中配置钉钉/飞书 Webhook 后可一键发送
            </div>
          )}
        </div>
      )}
    </div>
  );
}

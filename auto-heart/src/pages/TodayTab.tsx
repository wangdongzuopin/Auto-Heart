import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useMessageQueue } from '../hooks/useMessageQueue';
import { useSettings } from '../hooks/useSettings';

async function invokeWithTimeout<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error(`invoke "${cmd}" timeout after 5s`)), 5000)
  );
  return Promise.race([invoke<T>(cmd, args), timeout]) as Promise<T>;
}

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

interface ActivityCategoryStat {
  category: string;
  count: number;
  minutes: number;
}

interface ActivitySnapshotEntry {
  app_name: string;
  window_title: string;
  category: string;
  details: string;
  timestamp: string;
}

interface ActivitySessionStat {
  label: string;
  category: string;
  start_time: string;
  end_time: string;
  minutes: number;
}

interface TodayActivitySummary {
  total_active_minutes: number;
  total_idle_minutes: number;
  context_switches: number;
  categories: ActivityCategoryStat[];
  sessions: ActivitySessionStat[];
  snapshots: ActivitySnapshotEntry[];
}

const cardStyle: React.CSSProperties = {
  background: 'var(--color-background-secondary)',
  borderRadius: 'var(--border-radius-lg)',
  border: '0.5px solid var(--color-border-tertiary)',
  padding: 14,
};

export default function TodayTab() {
  const [tasks, setTasks] = useState<TodayTask[]>([]);
  const [intent, setIntent] = useState<{ raw_text: string; parsed: boolean } | null>(null);
  const [report, setReport] = useState<ReportData | null>(null);
  const [activitySummary, setActivitySummary] = useState<TodayActivitySummary>({
    total_active_minutes: 0,
    total_idle_minutes: 0,
    context_switches: 0,
    categories: [],
    sessions: [],
    snapshots: [],
  });
  const [editingReport, setEditingReport] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [sending, setSending] = useState<string | null>(null);
  const [sendResult, setSendResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const { messages, dismiss, ack } = useMessageQueue();
  const { settings } = useSettings();

  const loadTasks = useCallback(async () => {
    try {
      setTasks(await invokeWithTimeout<TodayTask[]>('get_today_tasks'));
    } catch {
      setTasks([]);
    }
  }, []);

  const loadIntent = useCallback(async () => {
    try {
      setIntent(await invokeWithTimeout<{ raw_text: string; parsed: boolean } | null>('get_today_intent'));
    } catch {
      setIntent(null);
    }
  }, []);

  const loadReport = useCallback(async () => {
    try {
      const nextReport = await invokeWithTimeout<ReportData | null>('get_today_report');
      setReport(nextReport);
      if (nextReport) setEditContent(nextReport.content);
    } catch {
      setReport(null);
    }
  }, []);

  const loadActivitySummary = useCallback(async () => {
    try {
      setActivitySummary(await invokeWithTimeout<TodayActivitySummary>('get_today_activity_summary'));
    } catch {
      setActivitySummary({
        total_active_minutes: 0,
        total_idle_minutes: 0,
        context_switches: 0,
        categories: [],
        sessions: [],
        snapshots: [],
      });
    }
  }, []);

  useEffect(() => {
    loadTasks();
    loadIntent();
    loadReport();
    loadActivitySummary();

    let unlistenParsed: (() => void) | undefined;
    let unlistenReport: (() => void) | undefined;
    let unlistenShallow: (() => void) | undefined;

    const setup = async () => {
      unlistenParsed = await listen('intent:parsed', () => {
        loadTasks();
        loadIntent();
        loadActivitySummary();
      });
      unlistenReport = await listen('daily_report:ready', () => loadReport());
      unlistenShallow = await listen('heartbeat:shallow', () => {
        loadIntent();
        loadActivitySummary();
      });
    };

    setup();
    return () => {
      unlistenParsed?.();
      unlistenReport?.();
      unlistenShallow?.();
    };
  }, [loadTasks, loadIntent, loadReport, loadActivitySummary]);

  const saveEdit = async () => {
    if (!report) return;
    try {
      await invokeWithTimeout('update_report_content', { date: report.date, content: editContent });
      setEditingReport(false);
      loadReport();
    } catch (error) {
      console.error(error);
    }
  };

  const sendReport = async (channel: 'dingtalk' | 'feishu') => {
    if (!report) return;
    setSending(channel);
    setSendResult(null);
    try {
      await invokeWithTimeout('send_daily_report', { date: report.date, channel });
      setSendResult({ ok: true, msg: channel === 'dingtalk' ? '已发送到钉钉' : '已发送到飞书' });
      loadReport();
    } catch (error) {
      setSendResult({ ok: false, msg: String(error) });
    } finally {
      setSending(null);
      setTimeout(() => setSendResult(null), 4000);
    }
  };

  const hasDingtalk = !!settings.dingtalkWebhook;
  const hasFeishu = !!settings.feishuWebhook;

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
      {intent && (
        <section style={cardStyle}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>今日意图</div>
            <span style={{ fontSize: 10, padding: '2px 8px', borderRadius: 999, background: intent.parsed ? 'rgba(47,133,90,0.12)' : 'var(--color-brand-light)', color: intent.parsed ? 'var(--color-text-success)' : 'var(--color-brand)' }}>
              {intent.parsed ? '已解析' : '待解析'}
            </span>
          </div>
          <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', lineHeight: 1.7, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
            {intent.raw_text}
          </div>
        </section>
      )}

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)', marginBottom: 8 }}>今日任务</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {tasks.length === 0 && (
            <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', textAlign: 'center', padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
              暂无解析出的任务
            </div>
          )}
          {tasks.map((task, index) => (
            <div key={index} style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 10px', background: task.status === 'active' ? 'var(--color-brand-light)' : 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)', border: task.status === 'active' ? '0.5px solid var(--color-brand-ring)' : '0.5px solid transparent', opacity: task.status === 'done' ? 0.55 : 1 }}>
              <span style={{ minWidth: 42, fontSize: 11, color: task.time ? 'var(--color-brand)' : 'var(--color-text-tertiary)', fontWeight: 600 }}>
                {task.time || '--'}
              </span>
              <span style={{ flex: 1, fontSize: 12, color: 'var(--color-text-primary)', textDecoration: task.status === 'done' ? 'line-through' : 'none' }}>
                {task.task}
              </span>
              {task.tag && (
                <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)', background: 'rgba(255,255,255,0.12)', padding: '2px 6px', borderRadius: 999 }}>
                  {task.tag}
                </span>
              )}
            </div>
          ))}
        </div>
      </section>

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)', marginBottom: 8 }}>今日电脑活动</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, minmax(0, 1fr))', gap: 8, marginBottom: 10 }}>
          <div style={{ padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>活跃时长</div>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--color-text-primary)' }}>{activitySummary.total_active_minutes} 分钟</div>
          </div>
          <div style={{ padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>空闲时长</div>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--color-text-primary)' }}>{activitySummary.total_idle_minutes} 分钟</div>
          </div>
          <div style={{ padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>切换次数</div>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--color-text-primary)' }}>{activitySummary.context_switches} 次</div>
          </div>
        </div>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 10 }}>
          {activitySummary.categories.map((item) => (
            <div key={item.category} style={{ padding: '7px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)', border: '0.5px solid var(--color-border-primary)' }}>
              <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>{item.category}</div>
              <div style={{ fontSize: 12, color: 'var(--color-text-primary)', fontWeight: 600 }}>{item.minutes} 分钟</div>
            </div>
          ))}
        </div>
        {activitySummary.snapshots.length === 0 ? (
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', textAlign: 'center', padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
            暂无前台活动快照
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {activitySummary.sessions.length > 0 && (
              <div>
                <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>连续工作段</div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {activitySummary.sessions.map((session, index) => (
                    <div key={`${session.start_time}-${index}`} style={{ padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)', border: '0.5px solid var(--color-border-primary)' }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 3 }}>
                        <span style={{ fontSize: 10, color: 'var(--color-brand)' }}>{session.category}</span>
                        <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>{session.start_time} - {session.end_time}</span>
                        <span style={{ marginLeft: 'auto', fontSize: 10, color: 'var(--color-text-secondary)' }}>{session.minutes} 分钟</span>
                      </div>
                      <div style={{ fontSize: 11, color: 'var(--color-text-primary)', wordBreak: 'break-word' }}>{session.label}</div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div>
              <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>最近活动</div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {activitySummary.snapshots.map((item, index) => (
              <div key={`${item.timestamp}-${index}`} style={{ padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                  <span style={{ fontSize: 10, color: 'var(--color-brand)' }}>{item.category}</span>
                  <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>{item.timestamp.slice(11, 16)}</span>
                </div>
                <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>{item.app_name}</div>
                {item.window_title && (
                  <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginTop: 2, wordBreak: 'break-word' }}>
                    {item.window_title}
                  </div>
                )}
              </div>
            ))}
              </div>
            </div>
          </div>
        )}
      </section>

      <section style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
          <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>待发送消息</div>
          {messages.length > 0 && (
            <span style={{ fontSize: 10, background: 'var(--color-brand)', color: '#fff', padding: '2px 7px', borderRadius: 999 }}>
              {messages.length}
            </span>
          )}
        </div>
        {messages.length === 0 ? (
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', textAlign: 'center', padding: '8px 10px', background: 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
            暂无待处理消息
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {messages.map((message) => (
              <div key={message.id} style={{ padding: '8px 10px', background: message.priority === 0 ? 'rgba(255,80,80,0.08)' : 'var(--color-background-tertiary)', borderRadius: 'var(--border-radius-md)' }}>
                <div style={{ fontSize: 11, fontWeight: 600, color: message.priority <= 1 ? 'var(--color-text-warning)' : 'var(--color-text-primary)', marginBottom: 3 }}>
                  {message.title}
                </div>
                <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', lineHeight: 1.6, marginBottom: 8 }}>
                  {message.content}
                </div>
                <div style={{ display: 'flex', gap: 6 }}>
                  <button onClick={() => ack(message.id)} style={{ padding: '5px 10px', fontSize: 11, borderRadius: 6, border: '0.5px solid var(--color-brand-ring)', background: 'var(--color-brand-light)', color: 'var(--color-brand)', cursor: 'pointer' }}>
                    帮我改
                  </button>
                  <button onClick={() => dismiss(message.id)} style={{ padding: '5px 10px', fontSize: 11, borderRadius: 6, border: '0.5px solid var(--color-border-primary)', background: 'transparent', color: 'var(--color-text-tertiary)', cursor: 'pointer' }}>
                    忽略
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {report && (
        <section style={{ ...cardStyle, border: report.status === 'sent' ? '0.5px solid rgba(47,133,90,0.3)' : '0.5px solid var(--color-brand-ring)' }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 10 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>今日日报</div>
              <span style={{ fontSize: 10, padding: '2px 8px', borderRadius: 999, background: report.status === 'sent' ? 'rgba(47,133,90,0.12)' : report.status === 'confirmed' ? 'var(--color-brand-light)' : 'rgba(255,176,32,0.12)', color: report.status === 'sent' ? 'var(--color-text-success)' : report.status === 'confirmed' ? 'var(--color-brand)' : 'var(--color-text-warning)' }}>
                {report.status === 'sent' ? '已发送' : report.status === 'confirmed' ? '已确认' : '草稿'}
              </span>
            </div>
            {report.status !== 'sent' && !editingReport && (
              <button onClick={() => { setEditingReport(true); setEditContent(report.content); }} style={{ padding: '4px 10px', fontSize: 11, borderRadius: 6, border: '0.5px solid var(--color-border-primary)', background: 'transparent', color: 'var(--color-text-secondary)', cursor: 'pointer' }}>
                编辑
              </button>
            )}
          </div>

          {editingReport ? (
            <div>
              <textarea
                value={editContent}
                onChange={(event) => setEditContent(event.target.value)}
                rows={8}
                style={{ width: '100%', boxSizing: 'border-box', padding: '10px 12px', fontSize: 12, lineHeight: 1.7, color: 'var(--color-text-primary)', background: 'var(--color-background-tertiary)', border: '0.5px solid var(--color-brand-ring)', borderRadius: 8, resize: 'vertical', outline: 'none' }}
              />
              <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                <button onClick={saveEdit} style={{ padding: '6px 12px', fontSize: 11, borderRadius: 6, border: 'none', background: 'var(--color-brand)', color: '#fff', cursor: 'pointer' }}>
                  保存修改
                </button>
                <button onClick={() => setEditingReport(false)} style={{ padding: '6px 12px', fontSize: 11, borderRadius: 6, border: '0.5px solid var(--color-border-primary)', background: 'transparent', color: 'var(--color-text-tertiary)', cursor: 'pointer' }}>
                  取消
                </button>
              </div>
            </div>
          ) : (
            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', lineHeight: 1.75, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
              {report.content}
            </div>
          )}

          {report.status !== 'sent' && !editingReport && (hasDingtalk || hasFeishu) && (
            <div style={{ marginTop: 12, display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              {hasDingtalk && (
                <button onClick={() => sendReport('dingtalk')} disabled={!!sending} style={{ padding: '6px 12px', fontSize: 11, borderRadius: 6, border: '0.5px solid var(--color-brand-ring)', background: 'var(--color-brand-light)', color: 'var(--color-brand)', cursor: sending ? 'default' : 'pointer', opacity: sending && sending !== 'dingtalk' ? 0.5 : 1 }}>
                  {sending === 'dingtalk' ? '发送中...' : '发到钉钉'}
                </button>
              )}
              {hasFeishu && (
                <button onClick={() => sendReport('feishu')} disabled={!!sending} style={{ padding: '6px 12px', fontSize: 11, borderRadius: 6, border: '0.5px solid var(--color-brand-ring)', background: 'var(--color-brand-light)', color: 'var(--color-brand)', cursor: sending ? 'default' : 'pointer', opacity: sending && sending !== 'feishu' ? 0.5 : 1 }}>
                  {sending === 'feishu' ? '发送中...' : '发到飞书'}
                </button>
              )}
            </div>
          )}

          {sendResult && (
            <div style={{ marginTop: 8, padding: '6px 8px', borderRadius: 6, fontSize: 11, color: sendResult.ok ? 'var(--color-text-success)' : '#e53e3e', background: sendResult.ok ? 'rgba(47,133,90,0.1)' : 'rgba(229,62,62,0.08)' }}>
              {sendResult.msg}
            </div>
          )}

          {report.status !== 'sent' && !hasDingtalk && !hasFeishu && (
            <div style={{ marginTop: 10, fontSize: 10, color: 'var(--color-text-tertiary)' }}>
              在设置里配置钉钉或飞书 Webhook 后，可以一键发送日报。
            </div>
          )}
        </section>
      )}
    </div>
  );
}

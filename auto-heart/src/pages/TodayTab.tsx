import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useSettings } from '../hooks/useSettings';

async function invokeWithTimeout<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error(`invoke "${cmd}" timeout after 5s`)), 5000),
  );
  return Promise.race([invoke<T>(cmd, args), timeout]) as Promise<T>;
}

interface ReportData {
  id: string;
  date: string;
  content: string;
  status: 'draft' | 'confirmed' | 'sent';
}

interface FileChangeEntry {
  path: string;
  change_type: 'create' | 'modify' | 'delete' | 'rename';
  timestamp: string;
}

interface TodayFileChanges {
  changes: FileChangeEntry[];
  total_count: number;
}

const cardStyle: React.CSSProperties = {
  background: 'var(--color-background-secondary)',
  borderRadius: 'var(--border-radius-lg)',
  border: '0.5px solid var(--color-border-tertiary)',
  padding: 14,
};

export default function TodayTab() {
  const [report, setReport] = useState<ReportData | null>(null);
  const [fileChanges, setFileChanges] = useState<TodayFileChanges>({ changes: [], total_count: 0 });
  const [editingReport, setEditingReport] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [sending, setSending] = useState<string | null>(null);
  const [sendResult, setSendResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [clearingChanges, setClearingChanges] = useState(false);
  const { settings } = useSettings();

  const loadReport = useCallback(async () => {
    try {
      const nextReport = await invokeWithTimeout<ReportData | null>('get_today_report');
      setReport(nextReport);
      if (nextReport) {
        setEditContent(nextReport.content);
      }
    } catch {
      setReport(null);
    }
  }, []);

  const loadFileChanges = useCallback(async () => {
    try {
      setFileChanges(await invokeWithTimeout<TodayFileChanges>('get_today_file_changes'));
    } catch {
      setFileChanges({ changes: [], total_count: 0 });
    }
  }, []);

  useEffect(() => {
    loadReport();
    loadFileChanges();

    let unlistenReport: (() => void) | undefined;
    let unlistenFileChange: (() => void) | undefined;

    const setup = async () => {
      unlistenReport = await listen('daily_report:ready', () => loadReport());
      unlistenFileChange = await listen('file:changed', () => loadFileChanges());
    };

    setup();
    return () => {
      unlistenReport?.();
      unlistenFileChange?.();
    };
  }, [loadReport, loadFileChanges]);

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

  const clearTodayChanges = async () => {
    if (clearingChanges) return;
    setClearingChanges(true);
    try {
      await invokeWithTimeout('clear_today_file_changes');
      await loadFileChanges();
    } catch (error) {
      console.error(error);
    } finally {
      setClearingChanges(false);
    }
  };

  const hasDingtalk = !!settings.dingtalkWebhook;
  const hasFeishu = !!settings.feishuWebhook;

  const changeTypeLabel: Record<FileChangeEntry['change_type'], string> = {
    create: '新建',
    modify: '修改',
    delete: '删除',
    rename: '重命名',
  };

  const changeTypeColor: Record<FileChangeEntry['change_type'], string> = {
    create: '#2f855a',
    modify: '#2b6cb0',
    delete: '#c53030',
    rename: '#c05621',
  };

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
      {/* 文件变更 */}
      <section style={cardStyle}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 8,
            marginBottom: 10,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>
              监听文件变更
            </div>
            <span
              style={{
                fontSize: 10,
                padding: '2px 7px',
                borderRadius: 999,
                background: 'var(--color-brand-light)',
                color: 'var(--color-brand)',
              }}
            >
              {fileChanges.total_count} 个变更
            </span>
          </div>
          <button
            type="button"
            onClick={clearTodayChanges}
            disabled={clearingChanges}
            style={{
              padding: '4px 10px',
              fontSize: 11,
              borderRadius: 6,
              border: '0.5px solid var(--color-border-primary)',
              background: 'transparent',
              color: 'var(--color-text-secondary)',
              cursor: clearingChanges ? 'default' : 'pointer',
              opacity: clearingChanges ? 0.6 : 1,
            }}
          >
            {clearingChanges ? '清空中...' : '清空'}
          </button>
        </div>

        {fileChanges.changes.length === 0 ? (
          <div
            style={{
              fontSize: 11,
              color: 'var(--color-text-tertiary)',
              textAlign: 'center',
              padding: '16px 12px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
            }}
          >
            暂无文件变更记录
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {fileChanges.changes.map((item, index) => (
              <div
                key={`${item.path}-${item.timestamp}-${index}`}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  padding: '8px 10px',
                  background: 'var(--color-background-tertiary)',
                  borderRadius: 'var(--border-radius-md)',
                }}
              >
                <span
                  style={{
                    fontSize: 9,
                    fontWeight: 700,
                    padding: '2px 6px',
                    borderRadius: 4,
                    background: `${changeTypeColor[item.change_type]}18`,
                    color: changeTypeColor[item.change_type],
                    flexShrink: 0,
                  }}
                >
                  {changeTypeLabel[item.change_type]}
                </span>
                <span
                  style={{
                    fontSize: 11,
                    color: 'var(--color-text-primary)',
                    wordBreak: 'break-all',
                    flex: 1,
                    minWidth: 0,
                  }}
                >
                  {item.path}
                </span>
                <span
                  style={{
                    fontSize: 10,
                    color: 'var(--color-text-tertiary)',
                    flexShrink: 0,
                  }}
                >
                  {item.timestamp.slice(11, 19)}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* 日报 */}
      {report && (
        <section
          style={{
            ...cardStyle,
            border:
              report.status === 'sent'
                ? '0.5px solid rgba(47,133,90,0.3)'
                : '0.5px solid var(--color-brand-ring)',
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              marginBottom: 10,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>
                今日日报
              </div>
              <span
                style={{
                  fontSize: 10,
                  padding: '2px 8px',
                  borderRadius: 999,
                  background:
                    report.status === 'sent'
                      ? 'rgba(47,133,90,0.12)'
                      : report.status === 'confirmed'
                        ? 'var(--color-brand-light)'
                        : 'rgba(255,176,32,0.12)',
                  color:
                    report.status === 'sent'
                      ? 'var(--color-text-success)'
                      : report.status === 'confirmed'
                        ? 'var(--color-brand)'
                        : 'var(--color-text-warning)',
                }}
              >
                {report.status === 'sent' ? '已发送' : report.status === 'confirmed' ? '已确认' : '草稿'}
              </span>
            </div>
            {report.status !== 'sent' && !editingReport && (
              <button
                onClick={() => {
                  setEditingReport(true);
                  setEditContent(report.content);
                }}
                style={{
                  padding: '4px 10px',
                  fontSize: 11,
                  borderRadius: 6,
                  border: '0.5px solid var(--color-border-primary)',
                  background: 'transparent',
                  color: 'var(--color-text-secondary)',
                  cursor: 'pointer',
                }}
              >
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
                style={{
                  width: '100%',
                  boxSizing: 'border-box',
                  padding: '10px 12px',
                  fontSize: 12,
                  lineHeight: 1.7,
                  color: 'var(--color-text-primary)',
                  background: 'var(--color-background-tertiary)',
                  border: '0.5px solid var(--color-brand-ring)',
                  borderRadius: 8,
                  resize: 'vertical',
                  outline: 'none',
                }}
              />
              <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                <button
                  onClick={saveEdit}
                  style={{
                    padding: '6px 12px',
                    fontSize: 11,
                    borderRadius: 6,
                    border: 'none',
                    background: 'var(--color-brand)',
                    color: '#fff',
                    cursor: 'pointer',
                  }}
                >
                  保存修改
                </button>
                <button
                  onClick={() => setEditingReport(false)}
                  style={{
                    padding: '6px 12px',
                    fontSize: 11,
                    borderRadius: 6,
                    border: '0.5px solid var(--color-border-primary)',
                    background: 'transparent',
                    color: 'var(--color-text-tertiary)',
                    cursor: 'pointer',
                  }}
                >
                  取消
                </button>
              </div>
            </div>
          ) : (
            <div
              style={{
                fontSize: 12,
                color: 'var(--color-text-secondary)',
                lineHeight: 1.75,
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
              }}
            >
              {report.content}
            </div>
          )}

          {report.status !== 'sent' && !editingReport && (hasDingtalk || hasFeishu) && (
            <div style={{ marginTop: 12, display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              {hasDingtalk && (
                <button
                  onClick={() => sendReport('dingtalk')}
                  disabled={!!sending}
                  style={{
                    padding: '6px 12px',
                    fontSize: 11,
                    borderRadius: 6,
                    border: '0.5px solid var(--color-brand-ring)',
                    background: 'var(--color-brand-light)',
                    color: 'var(--color-brand)',
                    cursor: sending ? 'default' : 'pointer',
                    opacity: sending && sending !== 'dingtalk' ? 0.5 : 1,
                  }}
                >
                  {sending === 'dingtalk' ? '发送中...' : '发到钉钉'}
                </button>
              )}
              {hasFeishu && (
                <button
                  onClick={() => sendReport('feishu')}
                  disabled={!!sending}
                  style={{
                    padding: '6px 12px',
                    fontSize: 11,
                    borderRadius: 6,
                    border: '0.5px solid var(--color-brand-ring)',
                    background: 'var(--color-brand-light)',
                    color: 'var(--color-brand)',
                    cursor: sending ? 'default' : 'pointer',
                    opacity: sending && sending !== 'feishu' ? 0.5 : 1,
                  }}
                >
                  {sending === 'feishu' ? '发送中...' : '发到飞书'}
                </button>
              )}
            </div>
          )}

          {sendResult && (
            <div
              style={{
                marginTop: 8,
                padding: '6px 8px',
                borderRadius: 6,
                fontSize: 11,
                color: sendResult.ok ? 'var(--color-text-success)' : '#e53e3e',
                background: sendResult.ok ? 'rgba(47,133,90,0.1)' : 'rgba(229,62,62,0.08)',
              }}
            >
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

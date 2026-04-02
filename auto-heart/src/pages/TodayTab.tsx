import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useMessageQueue } from '../hooks/useMessageQueue';
import { useSettings } from '../hooks/useSettings';

async function invokeWithTimeout<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error(`invoke "${cmd}" timeout after 5s`)), 5000),
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

const emptyActivitySummary: TodayActivitySummary = {
  total_active_minutes: 0,
  total_idle_minutes: 0,
  context_switches: 0,
  categories: [],
  sessions: [],
  snapshots: [],
};

const cardStyle: React.CSSProperties = {
  background: 'var(--color-background-secondary)',
  borderRadius: 'var(--border-radius-lg)',
  border: '0.5px solid var(--color-border-tertiary)',
  padding: 14,
};

const inputStyleLike: React.CSSProperties = {
  padding: '8px 10px',
  fontSize: 12,
  background: 'var(--color-background-tertiary)',
  border: '0.5px solid var(--color-border-primary)',
  borderRadius: 8,
  color: 'var(--color-text-primary)',
  boxSizing: 'border-box',
  outline: 'none',
};

const toolbarButtonStyle: React.CSSProperties = {
  padding: '5px 8px',
  fontSize: 10,
  borderRadius: 8,
  border: '0.5px solid var(--color-border-primary)',
  background: 'rgba(255,255,255,0.08)',
  color: 'var(--color-text-secondary)',
  cursor: 'pointer',
};

const dangerToolbarButtonStyle: React.CSSProperties = {
  ...toolbarButtonStyle,
  border: '0.5px solid rgba(229,62,62,0.25)',
  color: '#c53030',
  background: 'rgba(229,62,62,0.08)',
};

const primaryMiniButtonStyle: React.CSSProperties = {
  padding: '6px 10px',
  fontSize: 11,
  borderRadius: 8,
  border: 'none',
  background: 'var(--color-brand)',
  color: '#fff',
  cursor: 'pointer',
};

export default function TodayTab() {
  const [tasks, setTasks] = useState<TodayTask[]>([]);
  const [newTask, setNewTask] = useState('');
  const [newTaskTime, setNewTaskTime] = useState('');
  const [newTaskTag, setNewTaskTag] = useState('');
  const [taskFilter, setTaskFilter] = useState<'all' | 'pending' | 'active' | 'done'>('all');
  const [editingTaskIndex, setEditingTaskIndex] = useState<number | null>(null);
  const [editingTaskValue, setEditingTaskValue] = useState('');
  const [editingTaskTime, setEditingTaskTime] = useState('');
  const [editingTaskTag, setEditingTaskTag] = useState('');
  const [intent, setIntent] = useState<{ raw_text: string; parsed: boolean } | null>(null);
  const [report, setReport] = useState<ReportData | null>(null);
  const [activitySummary, setActivitySummary] = useState<TodayActivitySummary>(emptyActivitySummary);
  const [editingReport, setEditingReport] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [sending, setSending] = useState<string | null>(null);
  const [sendResult, setSendResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [clearingSnapshots, setClearingSnapshots] = useState(false);
  const [parsingIntent, setParsingIntent] = useState(false);
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
      if (nextReport) {
        setEditContent(nextReport.content);
      }
    } catch {
      setReport(null);
    }
  }, []);

  const loadActivitySummary = useCallback(async () => {
    try {
      setActivitySummary(await invokeWithTimeout<TodayActivitySummary>('get_today_activity_summary'));
    } catch {
      setActivitySummary(emptyActivitySummary);
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

  const clearTodaySnapshots = async () => {
    if (clearingSnapshots) return;
    const confirmed = window.confirm('确认清空今天的电脑活动快照吗？这会删除今天的前台窗口记录，但不会删除文件变更、日报和设置。');
    if (!confirmed) return;

    setClearingSnapshots(true);
    try {
      await invokeWithTimeout('clear_today_activity_snapshots');
      await loadActivitySummary();
    } catch (error) {
      console.error(error);
    } finally {
      setClearingSnapshots(false);
    }
  };

  const parseIntentNow = async () => {
    if (parsingIntent) return;
    setParsingIntent(true);
    try {
      await invokeWithTimeout<boolean>('parse_today_intent_now');
      await loadTasks();
      await loadIntent();
    } catch (error) {
      console.error(error);
    } finally {
      setParsingIntent(false);
    }
  };

  const addTask = async () => {
    const taskText = newTask.trim();
    if (!taskText) return;
    try {
      await invokeWithTimeout('add_today_task', {
        task: taskText,
        time: newTaskTime || null,
        tag: newTaskTag.trim() || null,
      });
      setNewTask('');
      setNewTaskTime('');
      setNewTaskTag('');
      await loadTasks();
      await loadIntent();
    } catch (error) {
      console.error(error);
    }
  };

  const cycleTaskStatus = async (index: number, currentStatus: TodayTask['status']) => {
    const nextStatus =
      currentStatus === 'pending' ? 'active' : currentStatus === 'active' ? 'done' : 'pending';
    try {
      await invokeWithTimeout('update_today_task_status', { index, status: nextStatus });
      await loadTasks();
    } catch (error) {
      console.error(error);
    }
  };

  const beginEditTask = (index: number, task: TodayTask) => {
    setEditingTaskIndex(index);
    setEditingTaskValue(task.task);
    setEditingTaskTime(task.time);
    setEditingTaskTag(task.tag);
  };

  const cancelEditTask = () => {
    setEditingTaskIndex(null);
    setEditingTaskValue('');
    setEditingTaskTime('');
    setEditingTaskTag('');
  };

  const saveTaskEdit = async () => {
    if (editingTaskIndex === null || !editingTaskValue.trim()) return;
    try {
      await invokeWithTimeout('update_today_task', {
        index: editingTaskIndex,
        task: editingTaskValue.trim(),
        time: editingTaskTime || null,
        tag: editingTaskTag.trim() || null,
      });
      cancelEditTask();
      await loadTasks();
    } catch (error) {
      console.error(error);
    }
  };

  const moveTask = async (index: number, direction: 'up' | 'down') => {
    try {
      await invokeWithTimeout('move_today_task', { index, direction });
      await loadTasks();
    } catch (error) {
      console.error(error);
    }
  };

  const removeTask = async (index: number) => {
    try {
      await invokeWithTimeout('delete_today_task', { index });
      if (editingTaskIndex === index) {
        cancelEditTask();
      }
      await loadTasks();
    } catch (error) {
      console.error(error);
    }
  };

  const hasDingtalk = !!settings.dingtalkWebhook;
  const hasFeishu = !!settings.feishuWebhook;
  const statusCounts = {
    pending: tasks.filter((task) => task.status === 'pending').length,
    active: tasks.filter((task) => task.status === 'active').length,
    done: tasks.filter((task) => task.status === 'done').length,
  };
  const visibleTasks = tasks.filter((task) => taskFilter === 'all' || task.status === taskFilter);
  const boardColumns: Array<{
    id: TodayTask['status'];
    label: string;
    accent: string;
    border: string;
    background: string;
    emptyText: string;
  }> = [
    {
      id: 'pending',
      label: '待完成',
      accent: '#c05621',
      border: 'rgba(237,137,54,0.24)',
      background: 'linear-gradient(180deg, rgba(237,137,54,0.10), rgba(237,137,54,0.03))',
      emptyText: '还没有待处理任务',
    },
    {
      id: 'active',
      label: '进行中',
      accent: '#2b6cb0',
      border: 'rgba(49,130,206,0.24)',
      background: 'linear-gradient(180deg, rgba(49,130,206,0.12), rgba(49,130,206,0.03))',
      emptyText: '还没有正在推进的任务',
    },
    {
      id: 'done',
      label: '已完成',
      accent: '#2f855a',
      border: 'rgba(72,187,120,0.24)',
      background: 'linear-gradient(180deg, rgba(72,187,120,0.12), rgba(72,187,120,0.03))',
      emptyText: '完成的任务会出现在这里',
    },
  ];

  const renderTaskCard = (task: TodayTask, index: number) => {
    const isEditing = editingTaskIndex === index;
    return (
      <div
        key={`${task.task}-${task.time}-${task.tag}-${index}`}
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 10,
          padding: '12px 12px',
          background:
            task.status === 'active'
              ? 'linear-gradient(135deg, rgba(49,130,206,0.18), rgba(49,130,206,0.08))'
              : task.status === 'done'
                ? 'linear-gradient(135deg, rgba(72,187,120,0.16), rgba(72,187,120,0.08))'
                : 'linear-gradient(135deg, rgba(237,137,54,0.14), rgba(237,137,54,0.07))',
          borderRadius: 14,
          border:
            task.status === 'active'
              ? '0.5px solid rgba(49,130,206,0.45)'
              : task.status === 'done'
                ? '0.5px solid rgba(72,187,120,0.4)'
                : '0.5px solid rgba(237,137,54,0.35)',
          boxShadow: task.status === 'active' ? '0 10px 24px rgba(49,130,206,0.10)' : 'none',
          opacity: task.status === 'done' ? 0.78 : 1,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <button
            onClick={() => cycleTaskStatus(index, task.status)}
            style={{
              minWidth: 78,
              padding: '6px 10px',
              borderRadius: 999,
              border: 'none',
              cursor: 'pointer',
              fontSize: 10,
              fontWeight: 700,
              letterSpacing: 0.2,
              background:
                task.status === 'active'
                  ? 'rgba(49,130,206,0.22)'
                  : task.status === 'done'
                    ? 'rgba(72,187,120,0.22)'
                    : 'rgba(237,137,54,0.2)',
              color:
                task.status === 'active'
                  ? '#2b6cb0'
                  : task.status === 'done'
                    ? '#2f855a'
                    : '#c05621',
            }}
          >
            {task.status === 'pending' ? '待完成' : task.status === 'active' ? '进行中' : '已完成'}
          </button>
          <span
            style={{
              minWidth: 44,
              fontSize: 11,
              color:
                task.status === 'done'
                  ? '#2f855a'
                  : task.status === 'active'
                    ? '#2b6cb0'
                    : '#c05621',
              fontWeight: 700,
            }}
          >
            {task.time || '--'}
          </span>
          {task.tag && (
            <span
              style={{
                fontSize: 10,
                color: 'var(--color-text-secondary)',
                background: 'rgba(255,255,255,0.18)',
                padding: '3px 8px',
                borderRadius: 999,
              }}
            >
              {task.tag}
            </span>
          )}
          <div style={{ marginLeft: 'auto', display: 'flex', gap: 6, flexWrap: 'wrap', justifyContent: 'flex-end' }}>
            <button onClick={() => moveTask(index, 'up')} style={toolbarButtonStyle}>上移</button>
            <button onClick={() => moveTask(index, 'down')} style={toolbarButtonStyle}>下移</button>
            <button onClick={() => beginEditTask(index, task)} style={toolbarButtonStyle}>编辑</button>
            <button onClick={() => removeTask(index)} style={dangerToolbarButtonStyle}>删除</button>
          </div>
        </div>
        {isEditing ? (
          <div style={{ display: 'grid', gridTemplateColumns: '84px minmax(0, 1fr) 110px auto auto', gap: 8 }}>
            <input
              type="time"
              value={editingTaskTime}
              onChange={(event) => setEditingTaskTime(event.target.value)}
              style={{ ...inputStyleLike, width: '100%' }}
            />
            <input
              type="text"
              value={editingTaskValue}
              onChange={(event) => setEditingTaskValue(event.target.value)}
              style={{ ...inputStyleLike, width: '100%' }}
            />
            <input
              type="text"
              value={editingTaskTag}
              onChange={(event) => setEditingTaskTag(event.target.value)}
              style={{ ...inputStyleLike, width: '100%' }}
            />
            <button onClick={saveTaskEdit} style={primaryMiniButtonStyle}>保存</button>
            <button onClick={cancelEditTask} style={toolbarButtonStyle}>取消</button>
          </div>
        ) : (
          <div
            style={{
              fontSize: 13,
              color: 'var(--color-text-primary)',
              textDecoration: task.status === 'done' ? 'line-through' : 'none',
              fontWeight: task.status === 'active' ? 700 : 600,
              lineHeight: 1.6,
              wordBreak: 'break-word',
            }}
          >
            {task.task}
          </div>
        )}
      </div>
    );
  };

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
      {intent && (
        <section style={cardStyle}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>今日意图</div>
            <span
              style={{
                fontSize: 10,
                padding: '2px 8px',
                borderRadius: 999,
                background: intent.parsed ? 'rgba(47,133,90,0.12)' : 'var(--color-brand-light)',
                color: intent.parsed ? 'var(--color-text-success)' : 'var(--color-brand)',
              }}
            >
              {intent.parsed ? '已解析' : '待解析'}
            </span>
            <button
              onClick={parseIntentNow}
              disabled={parsingIntent}
              style={{
                marginLeft: 'auto',
                padding: '4px 10px',
                fontSize: 11,
                borderRadius: 8,
                border: '0.5px solid var(--color-border-primary)',
                background: 'transparent',
                color: 'var(--color-text-secondary)',
                cursor: parsingIntent ? 'default' : 'pointer',
                opacity: parsingIntent ? 0.6 : 1,
              }}
            >
              {parsingIntent ? '解析中...' : '立即解析'}
            </button>
          </div>
          <div
            style={{
              fontSize: 12,
              color: 'var(--color-text-secondary)',
              lineHeight: 1.7,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
            }}
          >
            {intent.raw_text}
          </div>
        </section>
      )}

      <section style={cardStyle}>
        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)', marginBottom: 8 }}>
          今日任务
        </div>
        <div style={{ display: 'flex', gap: 8, marginBottom: 10, flexWrap: 'wrap' }}>
          {[
            { id: 'all' as const, label: '全部', count: tasks.length, color: 'var(--color-text-secondary)', bg: 'var(--color-background-tertiary)' },
            { id: 'pending' as const, label: '待完成', count: statusCounts.pending, color: '#c05621', bg: 'rgba(237,137,54,0.12)' },
            { id: 'active' as const, label: '进行中', count: statusCounts.active, color: '#2b6cb0', bg: 'rgba(49,130,206,0.14)' },
            { id: 'done' as const, label: '已完成', count: statusCounts.done, color: '#2f855a', bg: 'rgba(72,187,120,0.14)' },
          ].map((item) => (
            <button
              key={item.id}
              onClick={() => setTaskFilter(item.id)}
              style={{
                padding: '6px 10px',
                borderRadius: 999,
                border: taskFilter === item.id ? `1px solid ${item.color}` : '0.5px solid var(--color-border-primary)',
                background: taskFilter === item.id ? item.bg : 'transparent',
                color: item.color,
                cursor: 'pointer',
                fontSize: 11,
                fontWeight: 600,
              }}
            >
              {item.label} {item.count}
            </button>
          ))}
        </div>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: '80px minmax(0, 1fr) 110px auto',
            gap: 8,
            marginBottom: 10,
          }}
        >
          <input
            type="time"
            value={newTaskTime}
            onChange={(event) => setNewTaskTime(event.target.value)}
            style={{ ...inputStyleLike, width: '100%' }}
          />
          <input
            type="text"
            value={newTask}
            onChange={(event) => setNewTask(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && addTask()}
            placeholder="录入今天要做的任务"
            style={{ ...inputStyleLike, width: '100%' }}
          />
          <input
            type="text"
            value={newTaskTag}
            onChange={(event) => setNewTaskTag(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && addTask()}
            placeholder="标签/模块"
            style={{ ...inputStyleLike, width: '100%' }}
          />
          <button
            onClick={addTask}
            disabled={!newTask.trim()}
            style={{
              padding: '8px 12px',
              borderRadius: 10,
              border: 'none',
              background: newTask.trim() ? 'var(--color-brand)' : 'var(--color-background-tertiary)',
              color: newTask.trim() ? '#fff' : 'var(--color-text-tertiary)',
              cursor: newTask.trim() ? 'pointer' : 'default',
              fontSize: 12,
              fontWeight: 600,
            }}
          >
            添加
          </button>
        </div>
        {visibleTasks.length === 0 ? (
          <div
            style={{
              fontSize: 11,
              color: 'var(--color-text-tertiary)',
              textAlign: 'center',
              padding: '10px 12px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
            }}
          >
            {tasks.length === 0 ? '暂无解析出的任务' : '当前筛选下暂无任务'}
          </div>
        ) : (
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
              gap: 12,
              alignItems: 'start',
            }}
          >
            {boardColumns
              .filter((column) => taskFilter === 'all' || taskFilter === column.id)
              .map((column) => {
                const columnTasks = tasks
                  .map((task, index) => ({ task, index }))
                  .filter(({ task }) => task.status === column.id);

                return (
                  <div
                    key={column.id}
                    style={{
                      display: 'flex',
                      flexDirection: 'column',
                      gap: 10,
                      minHeight: 220,
                      padding: 12,
                      borderRadius: 18,
                      border: `1px solid ${column.border}`,
                      background: column.background,
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <span
                        style={{
                          width: 10,
                          height: 10,
                          borderRadius: '50%',
                          background: column.accent,
                          boxShadow: `0 0 0 4px ${column.border}`,
                        }}
                      />
                      <div style={{ fontSize: 12, fontWeight: 700, color: column.accent }}>{column.label}</div>
                      <span
                        style={{
                          marginLeft: 'auto',
                          minWidth: 24,
                          height: 24,
                          padding: '0 8px',
                          borderRadius: 999,
                          display: 'inline-flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                          fontSize: 11,
                          fontWeight: 700,
                          color: column.accent,
                          background: 'rgba(255,255,255,0.55)',
                        }}
                      >
                        {columnTasks.length}
                      </span>
                    </div>
                    {columnTasks.length === 0 ? (
                      <div
                        style={{
                          fontSize: 11,
                          color: 'var(--color-text-tertiary)',
                          textAlign: 'center',
                          padding: '24px 12px',
                          borderRadius: 14,
                          border: '1px dashed rgba(255,255,255,0.22)',
                          background: 'rgba(255,255,255,0.08)',
                        }}
                      >
                        {column.emptyText}
                      </div>
                    ) : (
                      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                        {columnTasks.map(({ task, index }) => renderTaskCard(task, index))}
                      </div>
                    )}
                  </div>
                );
              })}
          </div>
        )}
      </section>

      <section style={cardStyle}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 8,
            marginBottom: 8,
          }}
        >
          <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>
            今日电脑活动
          </div>
          <button
            type="button"
            onClick={clearTodaySnapshots}
            disabled={clearingSnapshots}
            style={{
              padding: '4px 10px',
              fontSize: 11,
              borderRadius: 6,
              border: '0.5px solid var(--color-border-primary)',
              background: 'transparent',
              color: 'var(--color-text-secondary)',
              cursor: clearingSnapshots ? 'default' : 'pointer',
              opacity: clearingSnapshots ? 0.6 : 1,
            }}
          >
            {clearingSnapshots ? '清空中...' : '清空今日快照'}
          </button>
        </div>

        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
            gap: 8,
            marginBottom: 10,
          }}
        >
          <div
            style={{
              padding: '8px 10px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
            }}
          >
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>活跃时长</div>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--color-text-primary)' }}>
              {activitySummary.total_active_minutes} 分钟
            </div>
          </div>
          <div
            style={{
              padding: '8px 10px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
            }}
          >
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>空闲时长</div>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--color-text-primary)' }}>
              {activitySummary.total_idle_minutes} 分钟
            </div>
          </div>
          <div
            style={{
              padding: '8px 10px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
            }}
          >
            <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>切换次数</div>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--color-text-primary)' }}>
              {activitySummary.context_switches} 次
            </div>
          </div>
        </div>

        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 10 }}>
          {activitySummary.categories.map((item) => (
            <div
              key={item.category}
              style={{
                padding: '7px 10px',
                background: 'var(--color-background-tertiary)',
                borderRadius: 'var(--border-radius-md)',
                border: '0.5px solid var(--color-border-primary)',
              }}
            >
              <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>{item.category}</div>
              <div style={{ fontSize: 12, color: 'var(--color-text-primary)', fontWeight: 600 }}>
                {item.minutes} 分钟
              </div>
            </div>
          ))}
        </div>

        {activitySummary.snapshots.length === 0 ? (
          <div
            style={{
              fontSize: 11,
              color: 'var(--color-text-tertiary)',
              textAlign: 'center',
              padding: '8px 10px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
            }}
          >
            暂无前台活动快照
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {activitySummary.sessions.length > 0 && (
              <div>
                <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>
                  连续工作段
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {activitySummary.sessions.map((session, index) => (
                    <div
                      key={`${session.start_time}-${index}`}
                      style={{
                        padding: '8px 10px',
                        background: 'var(--color-background-tertiary)',
                        borderRadius: 'var(--border-radius-md)',
                        border: '0.5px solid var(--color-border-primary)',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 3 }}>
                        <span style={{ fontSize: 10, color: 'var(--color-brand)' }}>{session.category}</span>
                        <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
                          {session.start_time} - {session.end_time}
                        </span>
                        <span
                          style={{
                            marginLeft: 'auto',
                            fontSize: 10,
                            color: 'var(--color-text-secondary)',
                          }}
                        >
                          {session.minutes} 分钟
                        </span>
                      </div>
                      <div
                        style={{
                          fontSize: 11,
                          color: 'var(--color-text-primary)',
                          wordBreak: 'break-word',
                        }}
                      >
                        {session.label}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div>
              <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>
                最近活动
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                {activitySummary.snapshots.map((item, index) => (
                  <div
                    key={`${item.timestamp}-${index}`}
                    style={{
                      padding: '8px 10px',
                      background: 'var(--color-background-tertiary)',
                      borderRadius: 'var(--border-radius-md)',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                      <span style={{ fontSize: 10, color: 'var(--color-brand)' }}>{item.category}</span>
                      <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
                        {item.timestamp.slice(11, 16)}
                      </span>
                    </div>
                    <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text-primary)' }}>
                      {item.app_name}
                    </div>
                    {item.details && item.details !== item.window_title && (
                      <div
                        style={{
                          fontSize: 11,
                          color: 'var(--color-text-primary)',
                          marginTop: 2,
                          wordBreak: 'break-word',
                        }}
                      >
                        {item.details}
                      </div>
                    )}
                    {item.window_title && (
                      <div
                        style={{
                          fontSize: 11,
                          color: 'var(--color-text-secondary)',
                          marginTop: 2,
                          wordBreak: 'break-word',
                        }}
                      >
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
            <span
              style={{
                fontSize: 10,
                background: 'var(--color-brand)',
                color: '#fff',
                padding: '2px 7px',
                borderRadius: 999,
              }}
            >
              {messages.length}
            </span>
          )}
        </div>
        {messages.length === 0 ? (
          <div
            style={{
              fontSize: 11,
              color: 'var(--color-text-tertiary)',
              textAlign: 'center',
              padding: '8px 10px',
              background: 'var(--color-background-tertiary)',
              borderRadius: 'var(--border-radius-md)',
            }}
          >
            暂无待处理消息
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {messages.map((message) => (
              <div
                key={message.id}
                style={{
                  padding: '8px 10px',
                  background:
                    message.priority === 0
                      ? 'rgba(255,80,80,0.08)'
                      : 'var(--color-background-tertiary)',
                  borderRadius: 'var(--border-radius-md)',
                }}
              >
                <div
                  style={{
                    fontSize: 11,
                    fontWeight: 600,
                    color:
                      message.priority <= 1
                        ? 'var(--color-text-warning)'
                        : 'var(--color-text-primary)',
                    marginBottom: 3,
                  }}
                >
                  {message.title}
                </div>
                <div
                  style={{
                    fontSize: 11,
                    color: 'var(--color-text-secondary)',
                    lineHeight: 1.6,
                    marginBottom: 8,
                  }}
                >
                  {message.content}
                </div>
                <div style={{ display: 'flex', gap: 6 }}>
                  <button
                    onClick={() => ack(message.id)}
                    style={{
                      padding: '5px 10px',
                      fontSize: 11,
                      borderRadius: 6,
                      border: '0.5px solid var(--color-brand-ring)',
                      background: 'var(--color-brand-light)',
                      color: 'var(--color-brand)',
                      cursor: 'pointer',
                    }}
                  >
                    帮我改
                  </button>
                  <button
                    onClick={() => dismiss(message.id)}
                    style={{
                      padding: '5px 10px',
                      fontSize: 11,
                      borderRadius: 6,
                      border: '0.5px solid var(--color-border-primary)',
                      background: 'transparent',
                      color: 'var(--color-text-tertiary)',
                      cursor: 'pointer',
                    }}
                  >
                    忽略
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

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

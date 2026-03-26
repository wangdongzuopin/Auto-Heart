import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
}

interface OperationLogEntry {
  id: string;
  timestamp: string;
  file_path: string;
  intention_desc: string;
  tags: string[];
}

export default function ConversationTab() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || loading) return;

    const userMsg: ChatMessage = {
      role: 'user',
      content: input.trim(),
      timestamp: new Date().toISOString(),
    };
    setMessages(prev => [...prev, userMsg]);
    setInput('');
    setLoading(true);

    try {
      const lower = userMsg.content.toLowerCase();
      let response: string;

      if (lower.includes('今天') && (lower.includes('做了') || lower.includes('做了什么'))) {
        const logs = await invoke<OperationLogEntry[]>('query_operation_logs', { date: 'today' });
        if (logs.length === 0) {
          response = '今天暂无记录的操作。可能还没有文件变更，或变更还未被分析。';
        } else {
          const summary = logs.slice(0, 10).map(l =>
            `- ${l.intention_desc} (${l.file_path})`
          ).join('\n');
          response = `今天共记录了 ${logs.length} 项操作：\n${summary}`;
        }
      } else if (lower.includes('找') && lower.includes('文件')) {
        const keyword = lower.replace(/.*找一下|文件/g, '').trim() || '%';
        const files = await invoke<[string, string, string][]>('search_file_changes', {
          keyword,
          daysBack: 7,
        });
        if (files.length === 0) {
          response = `最近7天没有找到包含"${keyword}"的文件变更。`;
        } else {
          const list = files.slice(0, 10).map(([path, type, time]) =>
            `- ${path} [${type}] @ ${time}`
          ).join('\n');
          response = `最近7天找到 ${files.length} 个相关文件：\n${list}`;
        }
      } else if (lower.includes('周') && (lower.includes('平均') || lower.includes('多少'))) {
        const stats = await invoke<{ avg_per_day: number; total_changes: number; top_modules: string[] }>('get_trend_stats', { days: 7 });
        response = `最近7天统计：\n- 总变更：${stats.total_changes} 次\n- 日均：${stats.avg_per_day.toFixed(1)} 次\n- 高频模块：${stats.top_modules.join(', ') || '无'}。`;
      } else {
        response = '我目前支持：\n- "今天我做了什么？" - 查询今日操作日志\n- "找一下 XX 文件" - 搜索文件变更\n- "这周平均多少" - 趋势统计\n\n请告诉我你想查询什么？';
      }

      setMessages(prev => [...prev, {
        role: 'assistant',
        content: response,
        timestamp: new Date().toISOString(),
      }]);
    } catch (err) {
      setMessages(prev => [...prev, {
        role: 'assistant',
        content: `抱歉，查询失败：${String(err)}`,
        timestamp: new Date().toISOString(),
      }]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ flex: 1, overflow: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
        {messages.length === 0 && (
          <div style={{ textAlign: 'center', color: 'var(--color-text-tertiary)', fontSize: 12, marginTop: 40 }}>
            问我关于今天的操作日志吧<br />
            <span style={{ fontSize: 11 }}>例如："今天我做了什么？"</span>
          </div>
        )}
        {messages.map((msg, i) => (
          <div key={i} style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: msg.role === 'user' ? 'flex-end' : 'flex-start',
          }}>
            <div style={{
              maxWidth: '80%',
              padding: '10px 14px',
              borderRadius: 12,
              background: msg.role === 'user'
                ? 'var(--color-brand-light)'
                : 'var(--color-background-secondary)',
              color: msg.role === 'user'
                ? 'var(--color-brand)'
                : 'var(--color-text-primary)',
              fontSize: 13,
              lineHeight: 1.5,
              whiteSpace: 'pre-wrap',
            }}>
              {msg.content}
            </div>
          </div>
        ))}
        {loading && (
          <div style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>思考中...</div>
        )}
        <div ref={messagesEndRef} />
      </div>

      <form onSubmit={handleSubmit} style={{
        padding: '12px 16px',
        borderTop: '0.5px solid var(--color-border-tertiary)',
        display: 'flex',
        gap: 8,
      }}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="问我关于今天的操作..."
          style={{
            flex: 1,
            padding: '8px 12px',
            fontSize: 13,
            background: 'var(--color-background-tertiary)',
            border: '0.5px solid var(--color-border-primary)',
            borderRadius: 8,
            color: 'var(--color-text-primary)',
            outline: 'none',
          }}
        />
        <button
          type="submit"
          disabled={loading || !input.trim()}
          style={{
            padding: '8px 16px',
            fontSize: 13,
            borderRadius: 8,
            background: loading ? 'var(--color-background-secondary)' : 'var(--color-brand)',
            border: 'none',
            color: '#fff',
            cursor: loading ? 'default' : 'pointer',
          }}
        >
          发送
        </button>
      </form>
    </div>
  );
}

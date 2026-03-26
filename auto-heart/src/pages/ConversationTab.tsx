import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
}

interface ConversationInfo {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  message_count: number;
}

interface OperationLogEntry {
  id: string;
  timestamp: string;
  file_path: string;
  intention_desc: string;
  tags: string[];
}

export default function ConversationTab() {
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [currentConvId, setCurrentConvId] = useState<string | null>(null);
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

  // 加载会话列表
  useEffect(() => {
    loadConversations();
  }, []);

  // 加载当前会话的消息
  useEffect(() => {
    if (currentConvId) {
      loadConversation(currentConvId);
    } else {
      setMessages([]);
    }
  }, [currentConvId]);

  const loadConversations = async () => {
    try {
      const list = await invoke<ConversationInfo[]>('get_conversations');
      setConversations(list);
    } catch (e) {
      console.error('[ConversationTab] load conversations:', e);
    }
  };

  const loadConversation = async (id: string) => {
    try {
      const conv = await invoke<{ messages: ChatMessage[] } | null>('get_conversation', { id });
      if (conv) {
        setMessages(conv.messages);
      }
    } catch (e) {
      console.error('[ConversationTab] load conversation:', e);
    }
  };

  const handleNewConversation = async () => {
    if (!input.trim()) return;
    try {
      const conv = await invoke<ConversationInfo>('create_conversation', { firstMessage: input.trim() });
      setCurrentConvId(conv.id);
      setMessages([{
        role: 'user',
        content: input.trim(),
        timestamp: new Date().toISOString(),
      }]);
      setInput('');
      await loadConversations();
    } catch (e) {
      console.error('[ConversationTab] create conversation:', e);
    }
  };

  const handleDeleteConversation = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke('delete_conversation', { id });
      if (currentConvId === id) {
        setCurrentConvId(null);
        setMessages([]);
      }
      await loadConversations();
    } catch (e) {
      console.error('[ConversationTab] delete:', e);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || loading) return;

    const userContent = input.trim();

    // 如果没有当前会话，先创建
    if (!currentConvId) {
      await handleNewConversationAndSend(userContent);
      return;
    }

    // 添加用户消息
    const userMsg: ChatMessage = {
      role: 'user',
      content: userContent,
      timestamp: new Date().toISOString(),
    };
    setMessages(prev => [...prev, userMsg]);
    setInput('');
    setLoading(true);

    try {
      const assistantMsg = await invoke<ChatMessage>('send_message', {
        sessionId: currentConvId,
        content: userContent,
      });
      setMessages(prev => [...prev, assistantMsg]);
    } catch (err) {
      setMessages(prev => [...prev, {
        role: 'assistant',
        content: `抱歉，发送失败：${String(err)}`,
        timestamp: new Date().toISOString(),
      }]);
    } finally {
      setLoading(false);
      await loadConversations();
    }
  };

  const handleNewConversationAndSend = async (content: string) => {
    setLoading(true);
    try {
      // 创建新会话
      const conv = await invoke<ConversationInfo>('create_conversation', { firstMessage: content });
      setCurrentConvId(conv.id);
      setMessages([{
        role: 'user',
        content: content,
        timestamp: new Date().toISOString(),
      }]);
      setInput('');

      // 发送消息
      const assistantMsg = await invoke<ChatMessage>('send_message', {
        sessionId: conv.id,
        content: content,
      });
      setMessages(prev => [...prev, assistantMsg]);
    } catch (err) {
      console.error('[ConversationTab] new conversation and send:', err);
    } finally {
      setLoading(false);
      await loadConversations();
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 会话列表 - 顶部横条 */}
      <div style={{
        display: 'flex',
        gap: 8,
        padding: '8px 12px',
        borderBottom: '0.5px solid var(--color-border-tertiary)',
        overflowX: 'auto',
        flexShrink: 0,
      }}>
        <button
          onClick={() => { setCurrentConvId(null); setMessages([]); }}
          style={{
            padding: '4px 12px',
            fontSize: 11,
            borderRadius: 12,
            background: !currentConvId ? 'var(--color-brand)' : 'var(--color-background-secondary)',
            border: 'none',
            color: !currentConvId ? '#fff' : 'var(--color-text-secondary)',
            cursor: 'pointer',
            whiteSpace: 'nowrap',
          }}
        >
          新对话
        </button>
        {conversations.map(conv => (
          <div
            key={conv.id}
            onClick={() => { setCurrentConvId(conv.id); }}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              padding: '4px 8px',
              fontSize: 11,
              borderRadius: 12,
              background: currentConvId === conv.id ? 'var(--color-brand)' : 'var(--color-background-secondary)',
              color: currentConvId === conv.id ? '#fff' : 'var(--color-text-secondary)',
              cursor: 'pointer',
              whiteSpace: 'nowrap',
              maxWidth: 120,
            }}
          >
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{conv.title}</span>
            <button
              onClick={(e) => handleDeleteConversation(conv.id, e)}
              style={{
                background: 'none',
                border: 'none',
                color: 'inherit',
                cursor: 'pointer',
                fontSize: 10,
                padding: 0,
                opacity: 0.6,
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>

      {/* 消息区域 */}
      <div style={{ flex: 1, overflow: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
        {messages.length === 0 && (
          <div style={{ textAlign: 'center', color: 'var(--color-text-tertiary)', fontSize: 12, marginTop: 40 }}>
            {currentConvId ? '开始对话吧' : '输入内容开始新对话'}
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

      {/* 输入框 */}
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
          placeholder={currentConvId ? '输入内容...' : '输入内容开始新对话...'}
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

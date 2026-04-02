import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

const LAST_CONVERSATION_KEY = 'auto-heart:last-conversation-id';

interface ChatMessage {
  id?: string;
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

interface ParsedAssistantContent {
  think: string | null;
  answer: string;
}

function parseAssistantContent(content: string): ParsedAssistantContent {
  const match = content.match(/<think>([\s\S]*?)<\/think>/i);
  if (!match) {
    return {
      think: null,
      answer: content.trim(),
    };
  }

  const think = match[1].trim();
  const answer = content.replace(match[0], '').trim();

  return {
    think: think || null,
    answer,
  };
}

function getMessageKey(message: ChatMessage, index: number) {
  return message.id ?? `${message.role}-${message.timestamp}-${index}`;
}

export default function ConversationTab() {
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [currentConvId, setCurrentConvId] = useState<string | null>(() => {
    try {
      return localStorage.getItem(LAST_CONVERSATION_KEY);
    } catch {
      return null;
    }
  });
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const parsedMessages = useMemo(
    () =>
      messages.map((message, index) => ({
        ...message,
        parsed: message.role === 'assistant' ? parseAssistantContent(message.content) : null,
        key: getMessageKey(message, index),
      })),
    [messages],
  );

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [parsedMessages, loading]);

  useEffect(() => {
    loadConversations();
  }, []);

  useEffect(() => {
    if (currentConvId) {
      try {
        localStorage.setItem(LAST_CONVERSATION_KEY, currentConvId);
      } catch {}
      loadConversation(currentConvId);
    } else {
      try {
        localStorage.removeItem(LAST_CONVERSATION_KEY);
      } catch {}
      setMessages([]);
    }
  }, [currentConvId]);

  const loadConversations = async () => {
    try {
      const list = await invoke<ConversationInfo[]>('get_conversations');
      setConversations(list);
      const rememberedId = (() => {
        try {
          return localStorage.getItem(LAST_CONVERSATION_KEY);
        } catch {
          return null;
        }
      })();

      if (!currentConvId && list.length > 0) {
        const nextConversation =
          (rememberedId && list.find((item) => item.id === rememberedId)) || list[0];
        if (nextConversation) {
          setCurrentConvId(nextConversation.id);
        }
      } else if (currentConvId && !list.some((item) => item.id === currentConvId)) {
        setCurrentConvId(list[0]?.id ?? null);
      }
    } catch (error) {
      console.error('[ConversationTab] load conversations:', error);
    }
  };

  const loadConversation = async (id: string) => {
    try {
      const conversation = await invoke<{ messages: ChatMessage[] } | null>('get_conversation', { id });
      if (conversation) {
        setMessages(conversation.messages);
      }
    } catch (error) {
      console.error('[ConversationTab] load conversation:', error);
    }
  };

  const handleDeleteConversation = async (id: string, event: React.MouseEvent) => {
    event.stopPropagation();
    try {
      await invoke('delete_conversation', { id });
      if (currentConvId === id) {
        setCurrentConvId(null);
        setMessages([]);
      }
      await loadConversations();
    } catch (error) {
      console.error('[ConversationTab] delete:', error);
    }
  };

  const sendAndAppend = async (sessionId: string, userContent: string) => {
    const assistantMessage = await invoke<ChatMessage>('send_message', {
      sessionId,
      content: userContent,
    });
    setMessages((previous) => [...previous, assistantMessage]);
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!input.trim() || loading) return;

    const userContent = input.trim();

    if (!currentConvId) {
      await handleNewConversationAndSend(userContent);
      return;
    }

    const userMessage: ChatMessage = {
      role: 'user',
      content: userContent,
      timestamp: new Date().toISOString(),
    };

    setMessages((previous) => [...previous, userMessage]);
    setInput('');
    setLoading(true);

    try {
      await sendAndAppend(currentConvId, userContent);
    } catch (error) {
      setMessages((previous) => [
        ...previous,
        {
          role: 'assistant',
          content: `发送失败：${String(error)}`,
          timestamp: new Date().toISOString(),
        },
      ]);
    } finally {
      setLoading(false);
      await loadConversations();
    }
  };

  const handleNewConversationAndSend = async (content: string) => {
    setLoading(true);
    try {
      const conversation = await invoke<ConversationInfo>('create_conversation', { firstMessage: content });
      setCurrentConvId(conversation.id);
      setMessages([
        {
          role: 'user',
          content,
          timestamp: new Date().toISOString(),
        },
      ]);
      setInput('');
      await sendAndAppend(conversation.id, content);
    } catch (error) {
      console.error('[ConversationTab] new conversation and send:', error);
    } finally {
      setLoading(false);
      await loadConversations();
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div
        style={{
          display: 'flex',
          gap: 8,
          padding: '8px 12px',
          borderBottom: '0.5px solid var(--color-border-tertiary)',
          overflowX: 'auto',
          flexShrink: 0,
        }}
      >
        <button
          onClick={() => {
            setCurrentConvId(null);
            setMessages([]);
          }}
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
        {conversations.map((conversation) => (
          <div
            key={conversation.id}
            onClick={() => {
              setCurrentConvId(conversation.id);
            }}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              padding: '4px 8px',
              fontSize: 11,
              borderRadius: 12,
              background:
                currentConvId === conversation.id
                  ? 'var(--color-brand)'
                  : 'var(--color-background-secondary)',
              color: currentConvId === conversation.id ? '#fff' : 'var(--color-text-secondary)',
              cursor: 'pointer',
              whiteSpace: 'nowrap',
              maxWidth: 140,
            }}
          >
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{conversation.title}</span>
            <button
              onClick={(event) => handleDeleteConversation(conversation.id, event)}
              aria-label={`删除对话 ${conversation.title}`}
              style={{
                background: 'none',
                border: 'none',
                color: 'inherit',
                cursor: 'pointer',
                fontSize: 12,
                padding: 0,
                opacity: 0.65,
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>

      <div
        style={{
          flex: 1,
          overflow: 'auto',
          padding: 16,
          display: 'flex',
          flexDirection: 'column',
          gap: 14,
        }}
      >
        {parsedMessages.length === 0 && (
          <div
            style={{
              textAlign: 'center',
              color: 'var(--color-text-tertiary)',
              fontSize: 12,
              marginTop: 40,
            }}
          >
            {currentConvId ? '开始对话吧' : '输入内容开始新对话'}
          </div>
        )}

        {parsedMessages.map((message) => {
          const isUser = message.role === 'user';
          const parsed = message.parsed;

          return (
            <div
              key={message.key}
              style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: isUser ? 'flex-end' : 'flex-start',
                gap: 8,
              }}
            >
              {isUser ? (
                <div
                  style={{
                    maxWidth: '78%',
                    padding: '12px 16px',
                    borderRadius: 18,
                    background: 'var(--color-brand-light)',
                    color: 'var(--color-brand)',
                    fontSize: 13,
                    lineHeight: 1.65,
                    whiteSpace: 'pre-wrap',
                    boxShadow: '0 8px 22px rgba(43, 108, 176, 0.08)',
                  }}
                >
                  {message.content}
                </div>
              ) : (
                <div className="chat-assistant-block">
                  {parsed?.answer && (
                    <div className="chat-assistant-bubble">{parsed.answer}</div>
                  )}
                </div>
              )}
            </div>
          );
        })}

        {loading && (
          <div style={{ display: 'flex', justifyContent: 'flex-start' }}>
            <div className="chat-loading-card">
              <div className="chat-loading-header">
                <span className="chat-think-pulse" aria-hidden="true">
                  <span />
                  <span />
                  <span />
                </span>
                <div>
                  <div className="chat-loading-title">思考中</div>
                  <div className="chat-loading-subtitle">正在读取监听数据、整理上下文并生成回答</div>
                </div>
              </div>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      <form
        onSubmit={handleSubmit}
        style={{
          padding: '12px 16px',
          borderTop: '0.5px solid var(--color-border-tertiary)',
          display: 'flex',
          gap: 8,
        }}
      >
        <input
          type="text"
          value={input}
          onChange={(event) => setInput(event.target.value)}
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
            minWidth: 88,
            padding: '8px 16px',
            fontSize: 13,
            borderRadius: 12,
            background: loading ? 'var(--color-background-secondary)' : 'var(--color-brand)',
            border: 'none',
            color: '#fff',
            cursor: loading ? 'default' : 'pointer',
            boxShadow: loading ? 'none' : '0 10px 24px rgba(43, 108, 176, 0.18)',
          }}
        >
          发送
        </button>
      </form>
    </div>
  );
}

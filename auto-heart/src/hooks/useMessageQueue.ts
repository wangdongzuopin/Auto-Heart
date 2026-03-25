import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface QueueMessage {
  id: string;
  priority: number;
  title: string;
  content: string;
  created_at: string;
}

/**
 * useMessageQueue — 实时消息队列 Hook
 *
 * - 初始化时从 DB 加载所有 pending 消息
 * - 监听 message_queue:new 事件，新消息实时追加
 * - 监听 message_queue:flush 事件，重新拉取队列
 */
export function useMessageQueue() {
  const [messages, setMessages] = useState<QueueMessage[]>([]);
  const [latest, setLatest] = useState<QueueMessage | null>(null);

  const fetchQueue = useCallback(async () => {
    try {
      const list = await invoke<QueueMessage[]>('get_message_queue');
      setMessages(list);
    } catch (err) {
      console.error('[useMessageQueue] fetch failed:', err);
    }
  }, []);

  const dismiss = useCallback(async (id: string) => {
    try {
      await invoke('dismiss_message', { id });
      setMessages((prev) => prev.filter((m) => m.id !== id));
      setLatest((prev) => (prev?.id === id ? null : prev));
    } catch (err) {
      console.error('[useMessageQueue] dismiss failed:', err);
    }
  }, []);

  const ack = useCallback(async (id: string) => {
    try {
      await invoke('ack_message', { id });
      setMessages((prev) => prev.filter((m) => m.id !== id));
      setLatest((prev) => (prev?.id === id ? null : prev));
    } catch (err) {
      console.error('[useMessageQueue] ack failed:', err);
    }
  }, []);

  useEffect(() => {
    fetchQueue();

    let unlistenNew: (() => void) | undefined;
    let unlistenFlush: (() => void) | undefined;

    const setup = async () => {
      unlistenNew = await listen<QueueMessage>('message_queue:new', (event) => {
        const msg = event.payload;
        setLatest(msg);
        setMessages((prev) => {
          if (prev.find((m) => m.id === msg.id)) return prev;
          return [msg, ...prev];
        });
      });

      unlistenFlush = await listen('message_queue:flush', () => {
        fetchQueue();
      });
    };

    setup();
    return () => {
      unlistenNew?.();
      unlistenFlush?.();
    };
  }, [fetchQueue]);

  return { messages, latest, dismiss, ack, refresh: fetchQueue };
}

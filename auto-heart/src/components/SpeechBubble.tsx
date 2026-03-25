import { useEffect, useState, useCallback } from 'react';

interface SpeechBubbleProps {
  message: {
    title: string;
    content: string;
  };
  onDismiss: () => void;
  onAction?: () => void;
  actionLabel?: string;   // 默认 "帮我改"
  dismissLabel?: string;  // 默认 "忽略"
}

/**
 * SpeechBubble — Orb 气泡通知
 *
 * 匹配 prototype 规范：
 * - 边框圆角 12px 12px 0 12px（右下角指向 Orb）
 * - 背景 #0a0a14 / 半透明 + 毛玻璃
 * - 边框 0.5px #AFA9EC
 * - 标题 #534AB7, 内容 #999
 * - 按钮：帮我改（品牌色）/ 忽略（灰色）
 * - 浮现动画 + 4秒自动收回
 */
export default function SpeechBubble({ message, onDismiss, onAction, actionLabel = '帮我改', dismissLabel = '忽略' }: SpeechBubbleProps) {
  const [visible, setVisible] = useState(false);

  const dismiss = useCallback(() => {
    setVisible(false);
    setTimeout(onDismiss, 300);
  }, [onDismiss]);

  useEffect(() => {
    const showTimer = setTimeout(() => setVisible(true), 50);
    const dismissTimer = setTimeout(dismiss, 4000);

    return () => {
      clearTimeout(showTimer);
      clearTimeout(dismissTimer);
    };
  }, [dismiss]);

  return (
    <div
      style={{
        position: 'absolute',
        bottom: '100%',
        right: 0,
        marginBottom: 8,
        opacity: visible ? 1 : 0,
        transform: visible ? 'translateY(0)' : 'translateY(6px)',
        transition: 'all 0.3s ease-out',
        pointerEvents: visible ? 'auto' : 'none',
        zIndex: 9999,
      }}
    >
      <div
        style={{
          background: 'rgba(10, 10, 20, 0.95)',
          border: '0.5px solid var(--color-brand-ring, #AFA9EC)',
          borderRadius: '12px 12px 0 12px',
          padding: '10px 14px',
          maxWidth: 220,
          minWidth: 160,
          backdropFilter: 'blur(12px)',
          WebkitBackdropFilter: 'blur(12px)',
          boxShadow: '0 4px 20px rgba(83, 74, 183, 0.2)',
        }}
      >
        {/* 标题 */}
        <div
          style={{
            fontSize: 11,
            color: 'var(--color-brand, #534AB7)',
            marginBottom: 4,
            fontWeight: 500,
          }}
        >
          {message.title}
        </div>

        {/* 内容 */}
        <div
          style={{
            fontSize: 12,
            color: 'var(--color-text-secondary, #999)',
            lineHeight: 1.5,
          }}
        >
          {message.content}
        </div>

        {/* 操作按钮 */}
        <div
          style={{
            display: 'flex',
            gap: 6,
            marginTop: 8,
          }}
        >
          <button
            onClick={(e) => {
              e.stopPropagation();
              onAction?.();
              dismiss();
            }}
            style={{
              fontSize: 11,
              padding: '3px 8px',
              borderRadius: 'var(--border-radius-sm, 5px)',
              cursor: 'pointer',
              background: 'var(--color-brand-light, #EEEDFE)',
              border: '0.5px solid var(--color-brand-ring, #AFA9EC)',
              color: 'var(--color-brand, #534AB7)',
              transition: 'background 0.15s',
            }}
            onMouseEnter={(e) => {
              (e.target as HTMLElement).style.background = '#d8d5ff';
            }}
            onMouseLeave={(e) => {
              (e.target as HTMLElement).style.background = '#EEEDFE';
            }}
          >
            {actionLabel}
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              dismiss();
            }}
            style={{
              fontSize: 11,
              padding: '3px 8px',
              borderRadius: 'var(--border-radius-sm, 5px)',
              cursor: 'pointer',
              background: 'transparent',
              border: '0.5px solid var(--color-text-tertiary, #666)',
              color: 'var(--color-text-secondary, #999)',
              transition: 'border-color 0.15s',
            }}
            onMouseEnter={(e) => {
              (e.target as HTMLElement).style.borderColor = '#999';
            }}
            onMouseLeave={(e) => {
              (e.target as HTMLElement).style.borderColor = '#666';
            }}
          >
            {dismissLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
'use client';

import type { ChatMessage as ChatMessageType } from '@/lib/types';

interface ChatMessageProps {
  message: ChatMessageType;
  isStreaming?: boolean;
}

export function ChatMessage({ message, isStreaming }: ChatMessageProps) {
  const isUser = message.role === 'user';

  return (
    <div style={{
      display: 'flex',
      justifyContent: isUser ? 'flex-end' : 'flex-start',
      marginBottom: '1.5rem',
    }}>
      <div style={{
        maxWidth: '75%',
        padding: '0.875rem 1.125rem',
        borderRadius: '12px',
        backgroundColor: isUser ? '#1e3a5f' : '#141414',
        border: isUser
          ? '1px solid rgba(59, 130, 246, 0.3)'
          : '1px solid #1a1a1a',
        color: '#e0e0e0',
        fontSize: '0.925rem',
        lineHeight: '1.6',
        whiteSpace: 'pre-wrap',
        wordBreak: 'break-word',
        fontFamily: isUser ? 'inherit' : 'inherit',
      }}>
        {message.content || (isStreaming ? (
          <span style={{
            display: 'inline-block',
            width: '8px',
            height: '16px',
            backgroundColor: '#888',
            animation: 'cursor-blink 1s step-end infinite',
          }} />
        ) : '')}
      </div>
    </div>
  );
}

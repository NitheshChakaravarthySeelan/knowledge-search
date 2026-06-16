'use client';

import { useState, useRef, useEffect, useCallback } from 'react';
import type { ChatMessage as ChatMessageType } from '@/lib/types';
import { askQuestion, searchDocuments, getSessionMessages } from '@/lib/api';
import { ChatMessage } from './ChatMessage';
import { SearchResultItem } from './SearchResultItem';

function generateId(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16);
  });
}

interface ChatThreadProps {
  sessionId: string | null;
  onSessionIdChange: (id: string) => void;
}

export function ChatThread({ sessionId, onSessionIdChange }: ChatThreadProps) {
  const [messages, setMessages] = useState<ChatMessageType[]>([]);
  const [input, setInput] = useState('');
  const [isWaiting, setIsWaiting] = useState(false);
  const [streamingAnswer, setStreamingAnswer] = useState('');
  const [showSearch, setShowSearch] = useState(false);
  const [searchResults, setSearchResults] = useState<any[]>([]);
  const [searchLatency, setSearchLatency] = useState<number | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const scrollToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [messages, streamingAnswer, scrollToBottom]);

  useEffect(() => {
    if (sessionId) {
      getSessionMessages(sessionId).then((msgs) => {
        setMessages(msgs);
      }).catch(() => {
        setMessages([]);
      });
    } else {
      setMessages([]);
    }
  }, [sessionId]);

  const handleSend = useCallback(async () => {
    const text = input.trim();
    if (!text || isWaiting) return;

    setInput('');
    setStreamingAnswer('');
    setSearchResults([]);
    setSearchLatency(null);
    setIsWaiting(true);

    // Add user message locally
    const userMsg: ChatMessageType = {
      id: `temp-${Date.now()}`,
      role: 'user',
      content: text,
      timestamp: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, userMsg]);

    // Generate local session ID if this is a new conversation
    const localSessionId = sessionId || generateId();
    if (!sessionId) {
      onSessionIdChange(localSessionId);
    }

    // If search toggle is on, also run search in parallel
    if (showSearch) {
      searchDocuments(text).then((data) => {
        setSearchResults(data.results);
        setSearchLatency(data.latency_ms);
      }).catch(() => {});
    }

    // Stream the answer
    let answerAccumulated = '';
    try {
      await askQuestion(text, 'default', (full) => {
        answerAccumulated = full;
        setStreamingAnswer(full);
      }, localSessionId);

      // After stream completes, add assistant message to local state
      if (answerAccumulated) {
        const assistantMsg: ChatMessageType = {
          id: `temp-${Date.now()}-a`,
          role: 'assistant',
          content: answerAccumulated,
          timestamp: new Date().toISOString(),
        };
        setMessages((prev) => [...prev, assistantMsg]);
        setStreamingAnswer('');
      }
    } catch {
      const errorMsg: ChatMessageType = {
        id: `temp-${Date.now()}-err`,
        role: 'assistant',
        content: 'Error: Cannot connect to Answer Engine. Please make sure agent-core is running on port 8001.',
        timestamp: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, errorMsg]);
      setStreamingAnswer('');
    } finally {
      setIsWaiting(false);
    }
  }, [input, isWaiting, sessionId, showSearch, onSessionIdChange]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div style={{
      flex: 1,
      display: 'flex',
      flexDirection: 'column',
      height: '100vh',
      backgroundColor: '#000000',
    }}>
      {/* Messages area */}
      <div style={{
        flex: 1,
        overflowY: 'auto',
        padding: '2rem 1.5rem',
      }}>
        {messages.length === 0 && !streamingAnswer && (
          <div style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            height: '100%',
            color: '#444',
          }}>
            <div style={{ fontSize: '2rem', marginBottom: '1rem', fontWeight: 300 }}>
              KnowledgeSearch
            </div>
            <div style={{ fontSize: '0.9rem', color: '#555' }}>
              Ask anything from your knowledge base
            </div>
          </div>
        )}

        {messages.map((msg) => (
          <ChatMessage key={msg.id} message={msg} />
        ))}

        {streamingAnswer && (
          <ChatMessage
            message={{
              id: 'streaming',
              role: 'assistant',
              content: streamingAnswer,
              timestamp: new Date().toISOString(),
            }}
            isStreaming={true}
          />
        )}

        {isWaiting && !streamingAnswer && (
          <div style={{
            display: 'flex',
            justifyContent: 'flex-start',
            marginBottom: '1.5rem',
          }}>
            <div style={{
              padding: '0.875rem 1.125rem',
              borderRadius: '12px',
              backgroundColor: '#141414',
              border: '1px solid #1a1a1a',
            }}>
              <div style={{ display: 'flex', gap: '4px' }}>
                <span className="typing-dot" />
                <span className="typing-dot" style={{ animationDelay: '0.2s' }} />
                <span className="typing-dot" style={{ animationDelay: '0.4s' }} />
              </div>
            </div>
          </div>
        )}

        {/* Search results inline */}
        {searchResults.length > 0 && (
          <div style={{
            marginTop: '0.5rem',
            marginBottom: '1rem',
            padding: '1rem',
            backgroundColor: '#0d0d0d',
            border: '1px solid #1a1a1a',
            borderRadius: '12px',
          }}>
            <div style={{
              fontSize: '0.75rem',
              color: '#555',
              marginBottom: '0.75rem',
              fontFamily: 'var(--font-mono)',
            }}>
              Search results ({searchLatency ? `${searchLatency}ms` : ''})
            </div>
            {searchResults.slice(0, 3).map((r: any) => (
              <SearchResultItem key={r.chunk_id} result={r} />
            ))}
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* Input area */}
      <div style={{
        borderTop: '1px solid #1a1a1a',
        padding: '1rem 1.5rem',
        backgroundColor: '#000000',
      }}>
        <div style={{
          display: 'flex',
          alignItems: 'flex-end',
          gap: '0.75rem',
          maxWidth: '900px',
          margin: '0 auto',
          width: '100%',
        }}>
          {/* Search toggle */}
          <button
            onClick={() => setShowSearch(!showSearch)}
            title="Show search results alongside answer"
            style={{
              background: 'none',
              border: showSearch ? '1px solid #3b82f6' : '1px solid #2a2a2a',
              borderRadius: '8px',
              color: showSearch ? '#3b82f6' : '#666',
              padding: '0.5rem',
              cursor: 'pointer',
              fontSize: '0.8rem',
              transition: 'all 0.15s',
              marginBottom: '0.5rem',
            }}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
          </button>

          <div style={{
            flex: 1,
            display: 'flex',
            gap: '0.5rem',
            alignItems: 'flex-end',
          }}>
            <textarea
              ref={inputRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Ask anything..."
              rows={1}
              style={{
                flex: 1,
                padding: '0.75rem 1rem',
                backgroundColor: '#0d0d0d',
                border: '1px solid #2a2a2a',
                borderRadius: '10px',
                color: '#e0e0e0',
                fontSize: '0.925rem',
                fontFamily: 'inherit',
                resize: 'none',
                outline: 'none',
                minHeight: '44px',
                maxHeight: '200px',
                transition: 'border-color 0.15s',
              }}
              onFocus={(e) => e.currentTarget.style.borderColor = '#555'}
              onBlur={(e) => e.currentTarget.style.borderColor = '#2a2a2a'}
              onInput={(e) => {
                const el = e.currentTarget;
                el.style.height = 'auto';
                el.style.height = Math.min(el.scrollHeight, 200) + 'px';
              }}
            />
            <button
              onClick={handleSend}
              disabled={!input.trim() || isWaiting}
              style={{
                padding: '0.625rem 1rem',
                backgroundColor: input.trim() && !isWaiting ? '#3b82f6' : '#1a1a1a',
                border: 'none',
                borderRadius: '10px',
                color: input.trim() && !isWaiting ? '#fff' : '#555',
                fontSize: '0.875rem',
                fontWeight: 600,
                cursor: input.trim() && !isWaiting ? 'pointer' : 'not-allowed',
                transition: 'all 0.15s',
                height: '44px',
                whiteSpace: 'nowrap',
              }}
            >
              Send
            </button>
          </div>
        </div>
        <div style={{
          textAlign: 'center',
          fontSize: '0.7rem',
          color: '#333',
          marginTop: '0.5rem',
        }}>
          KnowledgeSearch &middot; Hybrid RAG with entity-boosted retrieval
        </div>
      </div>

      <style>{`
        @keyframes cursor-blink {
          50% { opacity: 0; }
        }
        .typing-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background-color: #666;
          animation: dot-pulse 1.4s ease-in-out infinite;
        }
        @keyframes dot-pulse {
          0%, 60%, 100% { opacity: 0.3; transform: scale(0.8); }
          30% { opacity: 1; transform: scale(1); }
        }
      `}</style>
    </div>
  );
}

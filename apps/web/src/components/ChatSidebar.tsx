'use client';

import { useState, useEffect, useCallback } from 'react';
import type { SessionSummary } from '@/lib/types';
import { listSessions, deleteSession } from '@/lib/api';

interface ChatSidebarProps {
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  refreshTrigger: number;
}

export function ChatSidebar({
  activeSessionId,
  onSelectSession,
  onNewSession,
  refreshTrigger,
}: ChatSidebarProps) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const fetchSessions = useCallback(async () => {
    setLoading(true);
    try {
      const data = await listSessions();
      setSessions(data);
    } catch {
      // silently fail
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSessions();
  }, [fetchSessions, refreshTrigger]);

  const handleDelete = useCallback(async (id: string) => {
    try {
      await deleteSession(id);
      setSessions((prev) => prev.filter((s) => s.id !== id));
      if (activeSessionId === id) {
        onNewSession();
      }
    } catch {
      // silently fail
    }
    setConfirmDelete(null);
  }, [activeSessionId, onNewSession]);

  const formatTime = (ts: string) => {
    if (!ts) return '';
    const d = new Date(ts);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    return d.toLocaleDateString();
  };

  return (
    <div style={{
      width: '280px',
      minWidth: '280px',
      height: '100vh',
      backgroundColor: '#0a0a0a',
      borderRight: '1px solid #1a1a1a',
      display: 'flex',
      flexDirection: 'column',
      overflow: 'hidden',
    }}>
      <div style={{
        padding: '1rem',
        borderBottom: '1px solid #1a1a1a',
      }}>
        <button
          onClick={() => {
            onNewSession();
            fetchSessions();
          }}
          style={{
            width: '100%',
            padding: '0.75rem',
            backgroundColor: '#1a1a1a',
            border: '1px solid #2a2a2a',
            borderRadius: '8px',
            color: '#e0e0e0',
            fontSize: '0.875rem',
            fontWeight: 600,
            cursor: 'pointer',
            transition: 'background-color 0.15s',
          }}
          onMouseEnter={(e) => e.currentTarget.style.backgroundColor = '#222'}
          onMouseLeave={(e) => e.currentTarget.style.backgroundColor = '#1a1a1a'}
        >
          + New Chat
        </button>
      </div>

      <div style={{
        flex: 1,
        overflowY: 'auto',
        padding: '0.5rem',
      }}>
        {loading && sessions.length === 0 && (
          <div style={{ padding: '1rem', textAlign: 'center', color: '#555', fontSize: '0.8rem' }}>
            Loading...
          </div>
        )}

        {!loading && sessions.length === 0 && (
          <div style={{ padding: '1rem', textAlign: 'center', color: '#555', fontSize: '0.8rem' }}>
            No conversations yet
          </div>
        )}

        {sessions.map((session) => (
          <div
            key={session.id}
            onClick={() => {
              if (confirmDelete !== session.id) {
                onSelectSession(session.id);
              }
            }}
            style={{
              padding: '0.75rem',
              borderRadius: '8px',
              marginBottom: '0.25rem',
              cursor: 'pointer',
              backgroundColor: activeSessionId === session.id ? '#1a1a1a' : 'transparent',
              border: activeSessionId === session.id ? '1px solid #2a2a2a' : '1px solid transparent',
              transition: 'background-color 0.15s',
              position: 'relative',
            }}
            onMouseEnter={(e) => {
              if (activeSessionId !== session.id) {
                e.currentTarget.style.backgroundColor = '#111';
              }
            }}
            onMouseLeave={(e) => {
              if (activeSessionId !== session.id) {
                e.currentTarget.style.backgroundColor = 'transparent';
              }
            }}
          >
            <div style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'flex-start',
            }}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{
                  fontSize: '0.85rem',
                  color: '#e0e0e0',
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  fontWeight: activeSessionId === session.id ? 600 : 400,
                }}>
                  {session.preview || 'New conversation'}
                </div>
                <div style={{
                  fontSize: '0.7rem',
                  color: '#555',
                  marginTop: '0.2rem',
                }}>
                  {session.message_count} messages &middot; {formatTime(session.last_timestamp)}
                </div>
              </div>

              {confirmDelete === session.id ? (
                <div style={{ display: 'flex', gap: '0.25rem', marginLeft: '0.5rem' }}>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(session.id);
                    }}
                    style={{
                      background: 'none',
                      border: 'none',
                      color: '#ef4444',
                      fontSize: '0.7rem',
                      cursor: 'pointer',
                      padding: '0.15rem 0.3rem',
                    }}
                  >
                    Confirm
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirmDelete(null);
                    }}
                    style={{
                      background: 'none',
                      border: 'none',
                      color: '#888',
                      fontSize: '0.7rem',
                      cursor: 'pointer',
                      padding: '0.15rem 0.3rem',
                    }}
                  >
                    Cancel
                  </button>
                </div>
              ) : (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setConfirmDelete(session.id);
                  }}
                  style={{
                    background: 'none',
                    border: 'none',
                    color: '#555',
                    fontSize: '0.8rem',
                    cursor: 'pointer',
                    padding: '0.15rem 0.3rem',
                    opacity: 0,
                    transition: 'opacity 0.15s',
                  }}
                  className="sidebar-delete-btn"
                  onMouseEnter={(e) => e.currentTarget.style.opacity = '1'}
                >
                  ✕
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      <style>{`
        div:hover > .sidebar-delete-btn {
          opacity: 1 !important;
        }
      `}</style>
    </div>
  );
}

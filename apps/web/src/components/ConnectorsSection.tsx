'use client';

import { useState, useCallback } from 'react';
import type { Connector } from '@/lib/types';
import { syncConnector } from '@/lib/api';

const INITIAL_CONNECTORS: Connector[] = [
  {
    id: 'conn_notion_001',
    type: 'Notion',
    name: 'Company Knowledge Base',
    status: 'connected',
    last_sync: '2026-05-29T11:47:00Z',
  },
  {
    id: 'conn_slack_002',
    type: 'Slack',
    name: '#eng-announcements',
    status: 'disconnected',
    last_sync: null,
  },
];

export function ConnectorsSection() {
  const [connectors, setConnectors] =
    useState<Connector[]>(INITIAL_CONNECTORS);
  const [syncingId, setSyncingId] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);

  const handleSync = useCallback(
    async (id: string) => {
      setSyncingId(id);
      setSyncError(null);

      try {
        await syncConnector(id);
      } catch {
        setSyncError(`Failed to trigger sync for ${id}`);
      }

      setTimeout(() => {
        setConnectors((prev) =>
          prev.map((c) =>
            c.id === id
              ? {
                  ...c,
                  status: 'connected',
                  last_sync: new Date().toISOString(),
                }
              : c,
          ),
        );
        setSyncingId(null);
      }, 2500);
    },
    [],
  );

  return (
    <div className="glass-card">
      <h2 className="card-title">
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
        >
          <rect x="2" y="2" width="20" height="8" rx="2" ry="2" />
          <rect x="2" y="14" width="20" height="8" rx="2" ry="2" />
          <line x1="6" y1="6" x2="6.01" y2="6" />
          <line x1="6" y1="18" x2="6.01" y2="18" />
        </svg>
        Sync Connectors
      </h2>

      {syncError && (
        <p
          style={{
            fontSize: '0.8rem',
            color: 'var(--color-error)',
            marginBottom: '0.5rem',
          }}
        >
          {syncError}
        </p>
      )}

      <div className="sidebar-list">
        {connectors.map((conn) => (
          <div className="sidebar-item" key={conn.id}>
            <div className="sidebar-item-info">
              <span className="sidebar-item-title">{conn.name}</span>
              <span className="sidebar-item-desc">
                {conn.type} •{' '}
                {conn.last_sync
                  ? `Synced ${new Date(conn.last_sync).toLocaleTimeString()}`
                  : 'Never synced'}
              </span>
            </div>
            <div
              style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}
            >
              <span
                className={`pulse-dot ${conn.status === 'connected' ? 'active' : ''}`}
                style={{
                  backgroundColor:
                    conn.status === 'connected'
                      ? 'var(--color-success)'
                      : 'var(--text-muted)',
                }}
              />
              <button
                className={`sync-btn ${syncingId === conn.id ? 'loading' : ''}`}
                onClick={() => handleSync(conn.id)}
                disabled={syncingId !== null}
              >
                {syncingId === conn.id ? 'Syncing' : 'Sync'}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

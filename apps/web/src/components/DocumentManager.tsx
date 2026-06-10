'use client';

import { useState, useEffect, useCallback } from 'react';
import type { DocumentItem } from '@/lib/types';
import { listDocuments, deleteDocument, updateDocument } from '@/lib/api';

function MetadataEditor({
  metadata,
  onSave,
  onCancel,
}: {
  metadata: Record<string, unknown>;
  onSave: (m: Record<string, unknown>) => void;
  onCancel: () => void;
}) {
  const [entries, setEntries] = useState<[string, string][]>(() =>
    Object.entries(metadata).map(([k, v]) => [k, String(v)]),
  );

  const addEntry = () => setEntries((prev) => [...prev, ['', '']]);
  const removeEntry = (i: number) =>
    setEntries((prev) => prev.filter((_, idx) => idx !== i));
  const updateEntry = (i: number, field: 0 | 1, value: string) =>
    setEntries((prev) =>
      prev.map((e, idx) => (idx === i ? (field === 0 ? [value, e[1]] : [e[0], value]) : e)),
    );

  const handleSave = () => {
    const obj: Record<string, unknown> = {};
    for (const [k, v] of entries) {
      if (k.trim()) obj[k.trim()] = v;
    }
    onSave(obj);
  };

  return (
    <div
      style={{
        marginTop: '0.75rem',
        padding: '0.75rem',
        backgroundColor: 'rgba(255,255,255,0.03)',
        borderRadius: 'var(--radius-md)',
      }}
    >
      <div
        style={{
          fontSize: '0.8rem',
          fontWeight: 600,
          color: 'var(--text-secondary)',
          marginBottom: '0.5rem',
        }}
      >
        Edit Metadata
      </div>
      {entries.map(([k, v], i) => (
        <div
          key={i}
          style={{
            display: 'flex',
            gap: '0.5rem',
            marginBottom: '0.4rem',
            alignItems: 'center',
          }}
        >
          <input
            className="form-input"
            style={{ width: '40%', padding: '0.3rem 0.5rem', fontSize: '0.8rem' }}
            placeholder="Key"
            value={k}
            onChange={(e) => updateEntry(i, 0, e.target.value)}
          />
          <input
            className="form-input"
            style={{ flex: 1, padding: '0.3rem 0.5rem', fontSize: '0.8rem' }}
            placeholder="Value"
            value={v}
            onChange={(e) => updateEntry(i, 1, e.target.value)}
          />
          <button
            type="button"
            onClick={() => removeEntry(i)}
            style={{
              background: 'none',
              border: 'none',
              color: 'var(--color-error)',
              cursor: 'pointer',
              fontSize: '1rem',
            }}
          >
            ×
          </button>
        </div>
      ))}
      <div style={{ display: 'flex', gap: '0.5rem', marginTop: '0.5rem' }}>
        <button
          type="button"
          className="sync-btn"
          onClick={addEntry}
          style={{ fontSize: '0.75rem' }}
        >
          + Add Key
        </button>
        <button
          type="button"
          className="submit-btn"
          onClick={handleSave}
          style={{ fontSize: '0.75rem', padding: '0.3rem 0.75rem' }}
        >
          Save Metadata
        </button>
        <button
          type="button"
          className="sync-btn"
          onClick={onCancel}
          style={{ fontSize: '0.75rem' }}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

export function DocumentManager() {
  const [documents, setDocuments] = useState<DocumentItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const fetchDocuments = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await listDocuments();
      setDocuments(data.documents);
    } catch {
      setError('Failed to load documents.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDocuments();
  }, [fetchDocuments]);

  const handleDelete = useCallback(
    async (id: string) => {
      setDeletingId(id);
      try {
        await deleteDocument(id);
        setDocuments((prev) => prev.filter((d) => d.id !== id));
      } catch {
        setError('Failed to delete document.');
      } finally {
        setDeletingId(null);
        setConfirmDelete(null);
      }
    },
    [],
  );

  const handleSaveMetadata = useCallback(
    async (id: string, metadata: Record<string, unknown>) => {
      try {
        const updated = await updateDocument(id, { metadata });
        setDocuments((prev) =>
          prev.map((d) => (d.id === id ? { ...d, metadata: updated.metadata } : d)),
        );
        setEditingId(null);
      } catch {
        setError('Failed to update metadata.');
      }
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
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14 2 14 8 20 8" />
        </svg>
        Documents
        <button
          type="button"
          className="sync-btn"
          onClick={fetchDocuments}
          disabled={loading}
          style={{ marginLeft: 'auto', fontSize: '0.75rem' }}
        >
          {loading ? 'Refreshing...' : 'Refresh'}
        </button>
      </h2>

      {error && (
        <p
          style={{
            fontSize: '0.8rem',
            color: 'var(--color-error)',
            marginBottom: '0.5rem',
          }}
        >
          {error}
        </p>
      )}

      {loading && documents.length === 0 && (
        <div style={{ padding: '2rem', textAlign: 'center', color: 'var(--text-muted)' }}>
          Loading documents...
        </div>
      )}

      {!loading && documents.length === 0 && !error && (
        <div style={{ padding: '2rem', textAlign: 'center', color: 'var(--text-muted)' }}>
          No documents ingested yet. Use the ingestion form to add documents.
        </div>
      )}

      <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
        {documents.map((doc) => (
          <div
            key={doc.id}
            className="result-item"
            style={{ padding: '0.75rem' }}
          >
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'flex-start',
              }}
            >
              <div style={{ flex: 1 }}>
                <div className="result-title">{doc.title}</div>
                <div
                  style={{
                    fontSize: '0.75rem',
                    color: 'var(--text-muted)',
                    marginTop: '0.2rem',
                    fontFamily: 'var(--font-mono)',
                  }}
                >
                  {doc.status} •{' '}
                  {new Date(doc.created_at).toLocaleDateString()}
                  {doc.file_extension && ` • .${doc.file_extension}`}
                </div>
                <div
                  style={{
                    fontSize: '0.75rem',
                    color: 'var(--text-muted)',
                    marginTop: '0.1rem',
                  }}
                >
                  ID: {doc.id.slice(0, 8)}...
                </div>
              </div>
              <div style={{ display: 'flex', gap: '0.5rem' }}>
                <button
                  type="button"
                  className="sync-btn"
                  onClick={() =>
                    setEditingId(editingId === doc.id ? null : doc.id)
                  }
                  style={{ fontSize: '0.75rem' }}
                >
                  {editingId === doc.id ? 'Close' : 'Metadata'}
                </button>
                {confirmDelete === doc.id ? (
                  <>
                    <button
                      type="button"
                      className="submit-btn"
                      onClick={() => handleDelete(doc.id)}
                      disabled={deletingId === doc.id}
                      style={{
                        fontSize: '0.75rem',
                        padding: '0.3rem 0.5rem',
                        backgroundColor: 'var(--color-error)',
                      }}
                    >
                      {deletingId === doc.id ? '...' : 'Confirm'}
                    </button>
                    <button
                      type="button"
                      className="sync-btn"
                      onClick={() => setConfirmDelete(null)}
                      style={{ fontSize: '0.75rem' }}
                    >
                      Cancel
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="sync-btn"
                    onClick={() => setConfirmDelete(doc.id)}
                    style={{
                      fontSize: '0.75rem',
                      color: 'var(--color-error)',
                      borderColor: 'var(--color-error)',
                    }}
                  >
                    Delete
                  </button>
                )}
              </div>
            </div>

            {/* Metadata Display */}
            {editingId !== doc.id &&
              Object.keys(doc.metadata).length > 0 && (
                <div
                  style={{
                    marginTop: '0.5rem',
                    display: 'flex',
                    flexWrap: 'wrap',
                    gap: '0.25rem',
                  }}
                >
                  {Object.entries(doc.metadata as Record<string, unknown>).map(
                    ([k, v]) => (
                      <span
                        key={k}
                        className="badge"
                        style={{
                          backgroundColor: 'rgba(6, 182, 212, 0.1)',
                          color: 'var(--color-secondary)',
                          fontSize: '0.7rem',
                        }}
                      >
                        {k}: {String(v).slice(0, 50)}
                      </span>
                    ),
                  )}
                </div>
              )}

            {/* Metadata Editor */}
            {editingId === doc.id && (
              <MetadataEditor
                metadata={doc.metadata as Record<string, unknown>}
                onSave={(m) => handleSaveMetadata(doc.id, m)}
                onCancel={() => setEditingId(null)}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

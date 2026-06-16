'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { ingestDocument, getIngestionStatus } from '@/lib/api';

interface IngestionModalProps {
  open: boolean;
  onClose: () => void;
}

export function IngestionModal({ open, onClose }: IngestionModalProps) {
  const [docTitle, setDocTitle] = useState('');
  const [docContent, setDocContent] = useState('');
  const [isIngesting, setIsIngesting] = useState(false);
  const [ingestionPercent, setIngestionPercent] = useState(0);
  const [ingestionStage, setIngestionStage] = useState(0);
  const [ingestedId, setIngestedId] = useState<string | null>(null);
  const [ingestError, setIngestError] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [fileExtension, setFileExtension] = useState<string | null>(null);
  const [base64Content, setBase64Content] = useState('');
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const handleIngest = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!docTitle.trim()) return;
      if (!file && !docContent.trim()) return;

      setIsIngesting(true);
      setIngestionPercent(0);
      setIngestionStage(0);
      setIngestedId(null);
      setIngestError(null);

      const finalContent = file ? base64Content : docContent;

      try {
        const data = await ingestDocument(docTitle, finalContent, fileExtension ?? undefined);
        if (data.success) {
          setIngestedId(data.document_id);
        } else {
          setIsIngesting(false);
          setIngestError('Ingestion failed');
        }
      } catch {
        setIsIngesting(false);
        setIngestError('Failed to connect to backend');
      }
    },
    [docTitle, docContent, file, base64Content, fileExtension],
  );

  useEffect(() => {
    if (!isIngesting || !ingestedId) return;

    pollingRef.current = setInterval(async () => {
      try {
        const data = await getIngestionStatus(ingestedId);
        if ('error' in data) {
          clearInterval(pollingRef.current!);
          setIsIngesting(false);
          setIngestError('Job not found');
          return;
        }
        setIngestionStage(data.stage);
        setIngestionPercent(data.percent);

        if (data.status === 'completed' || data.status === 'failed') {
          clearInterval(pollingRef.current!);
          setIsIngesting(false);
          if (data.status === 'failed') {
            setIngestError(data.message || 'Ingestion failed');
          }
          // Auto close after completion
          setTimeout(() => {
            setIngestedId(null);
            setDocTitle('');
            setDocContent('');
            setFile(null);
            setFileExtension(null);
            setBase64Content('');
            onClose();
          }, 1500);
        }
      } catch {
        // continue polling
      }
    }, 2000);

    return () => {
      if (pollingRef.current) clearInterval(pollingRef.current);
    };
  }, [isIngesting, ingestedId, onClose]);

  useEffect(() => {
    if (!open) {
      setDocTitle('');
      setDocContent('');
      setIngestError(null);
      setIngestedId(null);
      setIsIngesting(false);
      setFile(null);
      setFileExtension(null);
      setBase64Content('');
    }
  }, [open]);

  // Handle Escape key
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !isIngesting) onClose();
    };
    if (open) {
      document.addEventListener('keydown', handler);
      return () => document.removeEventListener('keydown', handler);
    }
  }, [open, isIngesting, onClose]);

  if (!open) return null;

  return (
    <div
      onClick={(e) => {
        if (e.target === e.currentTarget && !isIngesting) onClose();
      }}
      style={{
        position: 'fixed',
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.8)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          backgroundColor: '#0d0d0d',
          border: '1px solid #2a2a2a',
          borderRadius: '16px',
          padding: '2rem',
          width: '520px',
          maxWidth: '90vw',
          maxHeight: '85vh',
          overflowY: 'auto',
        }}
      >
        {/* Header */}
        <div style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '1.5rem',
        }}>
          <h2 style={{
            fontSize: '1.125rem',
            fontWeight: 600,
            color: '#e0e0e0',
            margin: 0,
          }}>
            Ingest Document
          </h2>
          {!isIngesting && (
            <button
              onClick={onClose}
              style={{
                background: 'none',
                border: 'none',
                color: '#666',
                fontSize: '1.25rem',
                cursor: 'pointer',
                padding: '0.25rem',
                lineHeight: 1,
              }}
            >
              ✕
            </button>
          )}
        </div>

        <form onSubmit={handleIngest} style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
            <label style={{ fontSize: '0.85rem', fontWeight: 600, color: '#888' }}>
              Document Title
            </label>
            <input
              type="text"
              placeholder="e.g. Operating Guidelines v2"
              value={docTitle}
              onChange={(e) => setDocTitle(e.target.value)}
              disabled={isIngesting}
              required
              style={{
                padding: '0.75rem 1rem',
                backgroundColor: '#0a0a0a',
                border: '1px solid #2a2a2a',
                borderRadius: '8px',
                color: '#e0e0e0',
                fontSize: '0.925rem',
                fontFamily: 'inherit',
                outline: 'none',
              }}
              onFocus={(e) => e.currentTarget.style.borderColor = '#555'}
              onBlur={(e) => e.currentTarget.style.borderColor = '#2a2a2a'}
            />
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
            <label style={{ fontSize: '0.85rem', fontWeight: 600, color: '#888' }}>
              Upload File (PDF/DOCX)
            </label>
            <input
              type="file"
              accept=".pdf,.docx"
              disabled={isIngesting}
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) {
                  setFile(f);
                  const ext = f.name.split('.').pop()?.toLowerCase() ?? null;
                  setFileExtension(ext);
                  const reader = new FileReader();
                  reader.onload = () => {
                    const result = reader.result;
                    if (typeof result === 'string') {
                      const base64 = result.split(',')[1];
                      setBase64Content(base64);
                    }
                  };
                  reader.readAsDataURL(f);
                } else {
                  setFile(null);
                  setFileExtension(null);
                  setBase64Content('');
                }
              }}
              style={{
                padding: '0.5rem',
                color: '#888',
                fontSize: '0.85rem',
              }}
            />
          </div>

          {!file && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
              <label style={{ fontSize: '0.85rem', fontWeight: 600, color: '#888' }}>
                Raw Text Content
              </label>
              <textarea
                placeholder="Paste text contents here..."
                value={docContent}
                onChange={(e) => setDocContent(e.target.value)}
                disabled={isIngesting}
                required
                rows={6}
                style={{
                  padding: '0.75rem 1rem',
                  backgroundColor: '#0a0a0a',
                  border: '1px solid #2a2a2a',
                  borderRadius: '8px',
                  color: '#e0e0e0',
                  fontSize: '0.9rem',
                  fontFamily: 'var(--font-mono)',
                  resize: 'vertical',
                  outline: 'none',
                  minHeight: '120px',
                }}
                onFocus={(e) => e.currentTarget.style.borderColor = '#555'}
                onBlur={(e) => e.currentTarget.style.borderColor = '#2a2a2a'}
              />
            </div>
          )}

          <div style={{ display: 'flex', gap: '0.75rem', justifyContent: 'flex-end' }}>
            {!isIngesting && (
              <button
                type="button"
                onClick={onClose}
                style={{
                  padding: '0.625rem 1.25rem',
                  backgroundColor: '#1a1a1a',
                  border: '1px solid #2a2a2a',
                  borderRadius: '8px',
                  color: '#888',
                  fontSize: '0.875rem',
                  fontWeight: 500,
                  cursor: 'pointer',
                }}
              >
                Cancel
              </button>
            )}
            <button
              type="submit"
              disabled={isIngesting || !docTitle.trim() || (!file && !docContent.trim())}
              style={{
                padding: '0.625rem 1.25rem',
                backgroundColor: '#3b82f6',
                border: 'none',
                borderRadius: '8px',
                color: '#fff',
                fontSize: '0.875rem',
                fontWeight: 600,
                cursor: isIngesting ? 'not-allowed' : 'pointer',
                opacity: isIngesting ? 0.6 : 1,
              }}
            >
              {isIngesting ? 'Indexing...' : 'Index Document'}
            </button>
          </div>
        </form>

        {/* Ingest progress */}
        {isIngesting && (
          <div style={{
            marginTop: '1.5rem',
            padding: '1rem',
            backgroundColor: '#0a0a0a',
            borderRadius: '8px',
            border: '1px solid #1a1a1a',
          }}>
            <div style={{
              fontSize: '0.8rem',
              fontWeight: 600,
              color: '#888',
              marginBottom: '0.75rem',
            }}>
              Ingestion Progress: {ingestionPercent}%
            </div>
            <div style={{
              width: '100%',
              height: '6px',
              backgroundColor: '#1a1a1a',
              borderRadius: '3px',
              overflow: 'hidden',
            }}>
              <div style={{
                width: `${ingestionPercent}%`,
                height: '100%',
                backgroundColor: '#3b82f6',
                borderRadius: '3px',
                transition: 'width 0.3s ease',
              }} />
            </div>
            <p style={{
              fontSize: '0.8rem',
              color: '#666',
              marginTop: '0.75rem',
              fontFamily: 'var(--font-mono)',
            }}>
              Stage {ingestionStage}/4: Processing...
            </p>
          </div>
        )}

        {ingestError && (
          <p style={{
            fontSize: '0.8rem',
            color: '#ef4444',
            marginTop: '0.75rem',
          }}>
            {ingestError}
          </p>
        )}

        {!isIngesting && !ingestError && ingestedId && (
          <p style={{
            fontSize: '0.8rem',
            color: '#22c55e',
            marginTop: '0.75rem',
            fontFamily: 'var(--font-mono)',
          }}>
            ✓ Ingestion complete
          </p>
        )}
      </div>
    </div>
  );
}

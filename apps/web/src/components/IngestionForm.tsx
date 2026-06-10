'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { ingestDocument, getIngestionStatus } from '@/lib/api';

export function IngestionForm() {
  const [docTitle, setDocTitle] = useState('');
  const [docContent, setDocContent] = useState('');
  const [isIngesting, setIsIngesting] = useState(false);
  const [ingestionStep, setIngestionStep] = useState(0);
  const [ingestionPercent, setIngestionPercent] = useState(0);
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
      setIngestionStep(0);
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
        setIngestionStep(data.stage);
        setIngestionPercent(data.percent);

        if (data.status === 'completed' || data.status === 'failed') {
          clearInterval(pollingRef.current!);
          setIsIngesting(false);
          if (data.status === 'failed') {
            setIngestError(data.message || 'Ingestion failed');
          }
        }
      } catch {
        // continue polling
      }
    }, 2000);
    return () => {
      if (pollingRef.current) clearInterval(pollingRef.current);
    };
  }, [isIngesting, ingestedId]);

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
          <line x1="12" y1="18" x2="12" y2="12" />
          <line x1="9" y1="15" x2="15" y2="15" />
        </svg>
        Ingest Document Pipeline
      </h2>

      <form onSubmit={handleIngest} className="ingest-form">
        <div className="form-group">
          <label className="form-label">Document Title</label>
          <input
            type="text"
            className="form-input"
            placeholder="e.g. Operating Guidelines v2"
            value={docTitle}
            onChange={(e) => setDocTitle(e.target.value)}
            disabled={isIngesting}
            required
          />
        </div>

        <div className="form-group">
          <label className="form-label">Upload File (PDF/DOCX)</label>
          <input
            type="file"
            accept=".pdf,.docx"
            className="form-input"
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
            disabled={isIngesting}
          />
        </div>

        {!file && (
          <div className="form-group">
            <label className="form-label">Raw Text Content</label>
            <textarea
              className="form-input form-textarea"
              placeholder="Paste text contents here..."
              value={docContent}
              onChange={(e) => setDocContent(e.target.value)}
              disabled={isIngesting}
              required
            />
          </div>
        )}

        <button type="submit" className="submit-btn" disabled={isIngesting}>
          {isIngesting ? 'Processing Ingestion Pipeline...' : 'Index Document'}
        </button>
      </form>

      {ingestError && (
        <p
          style={{
            fontSize: '0.8rem',
            color: 'var(--color-error)',
            marginTop: '0.5rem',
          }}
        >
          {ingestError}
        </p>
      )}

      {isIngesting && (
        <div className="pipeline-stepper">
          <div
            style={{
              fontSize: '0.8rem',
              fontWeight: 600,
              color: 'var(--text-secondary)',
              marginBottom: '0.5rem',
            }}
          >
            Backend Ingestion: {ingestionPercent}%
          </div>
          <progress
            value={ingestionPercent}
            max="100"
            style={{ width: '100%', height: '8px' }}
          />
          <p
            style={{
              fontSize: '0.85rem',
              color: 'var(--color-secondary)',
              marginTop: '0.5rem',
              fontFamily: 'var(--font-mono)',
            }}
          >
            {ingestionStep > 0
              ? `Stage ${ingestionStep}/4: In progress...`
              : 'Connecting to pipeline...'}
          </p>
        </div>
      )}

      {!isIngesting && !ingestError && ingestedId && (
        <p
          style={{
            fontSize: '0.8rem',
            color: 'var(--color-success)',
            marginTop: '0.5rem',
            fontFamily: 'var(--font-mono)',
          }}
        >
          ✓ Ingestion complete for ID: {ingestedId}
        </p>
      )}
    </div>
  );
}

import type { SearchResult } from '@/lib/types';

export function SearchResultItem({ result }: { result: SearchResult }) {
  const meta = result.metadata as Record<string, unknown>;
  const source = typeof meta.source === 'string' ? meta.source : 'Source';
  const title = typeof meta.title === 'string' ? meta.title : 'Document Section';
  const url = typeof meta.url === 'string' ? meta.url : '#';

  return (
    <div className="result-item">
      <div className="result-header">
        <div className="result-title">{title}</div>
        <div className="result-meta">
          <span className="badge badge-score">
            {Math.round(result.score * 100)}% Match
          </span>
          <span className="badge badge-source">{source}</span>
        </div>
      </div>
      <p className="result-content">{result.content}</p>
      {url !== '#' && (
        <a
          href={url}
          target="_blank"
          rel="noreferrer"
          className="result-footer-link"
        >
          View Original Context →
        </a>
      )}
      {Object.keys(meta).length > 0 && (
        <div
          style={{
            marginTop: '0.5rem',
            display: 'flex',
            flexWrap: 'wrap',
            gap: '0.25rem',
          }}
        >
          {Object.entries(meta)
            .filter(
              ([k]) =>
                !['content', 'parent_content', 'document_id', 'title', 'source', 'url', 'index', 'start_offset', 'end_offset'].includes(k),
            )
            .slice(0, 4)
            .map(([k, v]) => (
              <span
                key={k}
                className="badge"
                style={{
                  backgroundColor: 'rgba(124, 58, 237, 0.15)',
                  color: 'var(--color-primary)',
                  fontSize: '0.7rem',
                }}
              >
                {k}: {String(v).slice(0, 40)}
              </span>
            ))}
        </div>
      )}
    </div>
  );
}

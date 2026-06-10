export function Header() {
  return (
    <header className="header-container">
      <div className="brand-section">
        <div className="brand-logo">K</div>
        <div>
          <h1 className="brand-title">Knowledge-OS</h1>
          <p style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
            Enterprise AI Bounded Infrastructure
          </p>
        </div>
        <span className="brand-tag">Rust-First</span>
      </div>

      <div style={{ display: 'flex', gap: '1.5rem', alignItems: 'center' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '0.5rem',
            fontSize: '0.85rem',
          }}
        >
          <span className="pulse-dot active" />
          <span style={{ color: 'var(--text-secondary)' }}>
            Ingestion Worker
          </span>
        </div>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '0.5rem',
            fontSize: '0.85rem',
          }}
        >
          <span className="pulse-dot active" />
          <span style={{ color: 'var(--text-secondary)' }}>
            Sync Worker
          </span>
        </div>
      </div>
    </header>
  );
}

export function MetricsSection() {
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
          <line x1="18" y1="20" x2="18" y2="10" />
          <line x1="12" y1="20" x2="12" y2="4" />
          <line x1="6" y1="20" x2="6" y2="14" />
        </svg>
        Infrastructure Metrics
      </h2>

      <div className="telemetry-grid">
        <div className="telemetry-block">
          <span className="telemetry-label">QDRANT VECTORS</span>
          <span className="telemetry-val">482</span>
        </div>
        <div className="telemetry-block">
          <span className="telemetry-label">POSTGRES RECORDS</span>
          <span className="telemetry-val">48</span>
        </div>
        <div className="telemetry-block">
          <span className="telemetry-label">AVG SEARCH LATENCY</span>
          <span className="telemetry-val">14.8ms</span>
        </div>
        <div className="telemetry-block">
          <span className="telemetry-label">ACTIVE TENANTS</span>
          <span className="telemetry-val">1</span>
        </div>
      </div>

      <div
        style={{
          marginTop: '1.5rem',
          fontSize: '0.8rem',
          color: 'var(--text-muted)',
          borderTop: '1px solid var(--border-color)',
          paddingTop: '1rem',
        }}
      >
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            marginBottom: '0.25rem',
          }}
        >
          <span>Docker Network Gateway</span>
          <span style={{ color: 'var(--color-success)', fontFamily: 'var(--font-mono)' }}>
            172.20.0.1
          </span>
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span>DB Engine Pool</span>
          <span style={{ color: 'var(--color-secondary)', fontFamily: 'var(--font-mono)' }}>
            5 / 20 connections
          </span>
        </div>
      </div>
    </div>
  );
}

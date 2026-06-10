export function SearchSkeleton() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
      {[1, 2, 3].map((i) => (
        <div
          key={i}
          className="result-item"
          style={{ pointerEvents: 'none' }}
        >
          <div className="result-header">
            <div
              style={{
                width: '60%',
                height: '1rem',
                backgroundColor: 'rgba(255,255,255,0.06)',
                borderRadius: '4px',
                animation: 'pulse-glow 1.5s ease-in-out infinite',
              }}
            />
            <div
              style={{
                width: '80px',
                height: '1rem',
                backgroundColor: 'rgba(255,255,255,0.06)',
                borderRadius: '4px',
                animation: 'pulse-glow 1.5s ease-in-out infinite',
                animationDelay: '0.1s',
              }}
            />
          </div>
          <div style={{ marginTop: '0.75rem', display: 'flex', flexDirection: 'column', gap: '0.4rem' }}>
            <div
              style={{
                width: '100%',
                height: '0.75rem',
                backgroundColor: 'rgba(255,255,255,0.06)',
                borderRadius: '4px',
                animation: 'pulse-glow 1.5s ease-in-out infinite',
                animationDelay: '0.2s',
              }}
            />
            <div
              style={{
                width: '80%',
                height: '0.75rem',
                backgroundColor: 'rgba(255,255,255,0.06)',
                borderRadius: '4px',
                animation: 'pulse-glow 1.5s ease-in-out infinite',
                animationDelay: '0.3s',
              }}
            />
            <div
              style={{
                width: '60%',
                height: '0.75rem',
                backgroundColor: 'rgba(255,255,255,0.06)',
                borderRadius: '4px',
                animation: 'pulse-glow 1.5s ease-in-out infinite',
                animationDelay: '0.4s',
              }}
            />
          </div>
        </div>
      ))}
    </div>
  );
}

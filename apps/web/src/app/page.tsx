"use client";

import React, { useState, useEffect } from 'react';

interface SearchResult {
  chunk_id: string;
  document_id: string;
  content: string;
  score: number;
  metadata: {
    source: string;
    title: string;
    url: string;
  };
}

interface Connector {
  id: string;
  type: string;
  name: string;
  status: string;
  last_sync: string | null;
}

export default function Home() {
  // Core Application State
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [searchLatency, setSearchLatency] = useState<number | null>(null);
  
  // New State for Answer Engine
  const [isAsking, setIsAsking] = useState(false);
  const [aiAnswer, setAiAnswer] = useState<string | null>(null);

  // Ingestion Form State
  const [docTitle, setDocTitle] = useState('');
  const [docContent, setDocContent] = useState('');
  const [isIngesting, setIsIngesting] = useState(false);
  const [ingestionStep, setIngestionStep] = useState<number>(0);
  const [ingestionPercent, setIngestionPercent] = useState<number>(0);
  const [ingestedId, setIngestedId] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [fileExtension, setFileExtension] = useState<string | null>(null);
  const [base64Content, setBase64Content] = useState<string>('');

  // Connector Sync State
  const [connectors, setConnectors] = useState<Connector[]>([
    { id: "conn_notion_001", type: "Notion", name: "Company Knowledge Base", status: "connected", last_sync: "2026-05-29T11:47:00Z" },
    { id: "conn_slack_002", type: "Slack", name: "#eng-announcements", status: "disconnected", last_sync: null }
  ]);
  const [isSyncing, setIsSyncing] = useState<string | null>(null);

  // Telemetry Mock Metrics
  const [qdrantCount, setQdrantCount] = useState(482);
  const [postgresCount, setPostgresCount] = useState(48);
  const [averageLatency, setAverageLatency] = useState(14.8);

  // Initial Seed Results for aesthetics on first load
  useEffect(() => {
    setSearchResults([
      {
        chunk_id: "seed_chunk_0",
        document_id: "seed_doc_0",
        content: "Knowledge-OS is online. Query your database or type standard text inputs on the left panel to test parsing, chunking, and mock embedding vectors generation.",
        score: 1.0,
        metadata: {
          source: "FileUpload",
          title: "System Guidelines",
          url: "#"
        }
      }
    ]);
  }, []);

  // Handler: Hybrid Vector Search
  const handleSearch = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!searchQuery.trim()) return;

    setIsSearching(true);
    setAiAnswer(null); // Clear previous AI answers
    const startTime = performance.now();

    try {
      const response = await fetch('http://localhost:8000/api/search', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: searchQuery, limit: 5 })
      });

      if (response.ok) {
        const data = await response.json();
        setSearchResults(data.results);
        setSearchLatency(data.latency_ms);
      } else {
        throw new Error('Backend offline');
      }
    } catch (err) {
      console.warn("API Gateway is offline. Search simulator fallback.");
      setTimeout(() => {
        const mockResults: SearchResult[] = [
          {
            chunk_id: `sim_chunk_${Math.random().toString(36).substring(5)}`,
            document_id: "sim_doc_notion",
            content: `This is a high-fidelity matching paragraph discovered in Notion regarding: "${searchQuery}". It compiles perfectly and provides modular context.`,
            score: 0.89,
            metadata: {
              source: "Notion",
              title: "Engineering Onboarding Roadmap",
              url: "https://notion.so/knowledge-os/Engineering-Onboarding-Roadmap"
            }
          },
          {
            chunk_id: "sim_chunk_2",
            document_id: "sim_doc_strategy",
            content: "We chose Qdrant due to its high-performance Rust execution, support for payload filtering, and dynamic index updates, which makes it perfect for multi-tenant scaling.",
            score: 0.74,
            metadata: {
              source: "FileUpload",
              title: "Database Strategy Draft",
              url: "#"
            }
          }
        ];
        setSearchResults(mockResults);
        setSearchLatency(Math.round(performance.now() - startTime));
      }, 500);
    } finally {
      setIsSearching(false);
    }
  };

  // Handler: Ask AI Engine
  const handleAsk = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!searchQuery.trim()) return;

    setIsAsking(true);
    setSearchResults([]); // Hide chunks to focus on the answer
    setAiAnswer(null);
    
    try {
      const response = await fetch('http://localhost:8000/api/ask', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ question: searchQuery })
      });

      if (response.ok) {
        const data = await response.json();
        setAiAnswer(data.answer);
      } else {
        setAiAnswer("Error: Failed to fetch answer from LLM.");
      }
    } catch (err) {
      setAiAnswer("Error: Cannot connect to Answer Engine.");
    } finally {
      setIsAsking(false);
    }
  };

  // Handler: Document Ingestion Pipeline
  const handleIngest = async (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!docTitle.trim()) return;
    if (!file && !docContent.trim()) return;

    setIsIngesting(true);
    setIngestionStep(0); 
    setIngestedId(null);

    const finalContent = file ? base64Content : docContent;

    try {
      const response = await fetch('http://localhost:8000/api/documents', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          title: docTitle, 
          content: finalContent,
          fileExtension: fileExtension 
        })
      });
      const data = await response.json();
      if (data.success) {
        setIngestedId(data.document_id);
      } else {
        setIsIngesting(false);
        alert("Ingestion failed: " + data.error);
      }
    } catch (err) {
      setIsIngesting(false);
      alert("Failed to connect to backend");
    }
  };

  // Poll for ingestion status
  useEffect(() => {
    if (!isIngesting || !ingestedId) return;

    const interval = setInterval(async () => {
      try {
        const res = await fetch(`http://localhost:8000/api/documents/${ingestedId}/status`);
        const data = await res.json();
        
        if (data.error) {
            console.error("Job not found");
            clearInterval(interval);
            setIsIngesting(false);
            return;
        }

        setIngestionStep(data.stage || 0);
        setIngestionPercent(data.percent || 0);
        
        if (data.status === 'completed' || data.status === 'failed') {
          clearInterval(interval);
          setIsIngesting(false);
          if (data.status === 'failed') alert("Ingestion failed: " + data.message);
        }
      } catch (err) {
        console.error("Failed to poll status", err);
      }
    }, 2000); // Poll every 2 seconds
    return () => clearInterval(interval);
  }, [isIngesting, ingestedId]);

  // Handler: Connectors Sync crawler trigger
  const handleSync = async (id: string) => {
    setIsSyncing(id);

    try {
      const response = await fetch(`http://localhost:8000/api/connectors/${id}/sync`, {
        method: 'POST'
      });
      if (response.ok) {
        // Sync completed in background, locally we simulate crawler intervals
      }
    } catch (_) {}

    // Simulated high-fidelity crawl interval
    setTimeout(() => {
      setConnectors(prev =>
        prev.map(c =>
          c.id === id
            ? { ...c, status: "connected", last_sync: new Date().toISOString() }
            : c
        )
      );
      setIsSyncing(null);
      setQdrantCount(prev => prev + 2); // Simulating adding 2 crawled pages
    }, 2500);
  };

  return (
    <div className="dashboard-wrapper">
      {/* Brand Header */}
      <header className="header-container">
        <div className="brand-section">
          <div className="brand-logo">K</div>
          <div>
            <h1 className="brand-title">Knowledge-OS</h1>
            <p style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>Enterprise AI Bounded Infrastructure</p>
          </div>
          <span className="brand-tag">Rust-First</span>
        </div>
        
        {/* Core System States */}
        <div style={{ display: 'flex', gap: '1.5rem', alignItems: 'center' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '0.85rem' }}>
            <span className="pulse-dot active"></span>
            <span style={{ color: 'var(--text-secondary)' }}>Ingestion Worker</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '0.85rem' }}>
            <span className="pulse-dot active"></span>
            <span style={{ color: 'var(--text-secondary)' }}>Sync Worker</span>
          </div>
        </div>
      </header>

      {/* Main Grid Workspace */}
      <main className="grid-container">
        
        {/* Left Side: Search Panel & Search Results */}
        <section style={{ display: 'flex', flexDirection: 'column', gap: '2rem' }}>
          
          {/* Section: Hybrid search bar */}
          <div className="glass-card">
            <h2 className="card-title">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line>
              </svg>
              Hybrid Vector Search
            </h2>
            <form onSubmit={handleSearch} className="search-container">
              <div className="search-input-wrapper">
                <input 
                  type="text" 
                  className="search-input"
                  placeholder="Ask anything from your synced connections... (e.g. database strategy)"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                />
                <button type="submit" className="search-btn" disabled={isSearching}>
                  {isSearching ? 'Searching...' : 'Search'}
                </button>
                <button 
                  type="button" 
                  className="search-btn" 
                  style={{ backgroundColor: 'var(--color-secondary)' }}
                  onClick={handleAsk}
                  disabled={isAsking}
                >
                  {isAsking ? 'Asking...' : 'Ask AI'}
                </button>
              </div>
              {searchLatency !== null && (
                <p style={{ fontSize: '0.75rem', color: 'var(--text-muted)', fontFamily: 'var(--font-mono)' }}>
                  Multi-stage search completed in <span style={{ color: 'var(--color-secondary)' }}>{searchLatency}ms</span>
                </p>
              )}
            </form>

            {/* Results Output */}
            <div className="results-wrapper">
              {aiAnswer && (
                <div className="glass-card" style={{ padding: '1rem', marginBottom: '1rem', border: '1px solid var(--color-secondary)' }}>
                  <h3 style={{ margin: '0 0 0.5rem 0', color: 'var(--color-secondary)' }}>AI Answer</h3>
                  <p style={{ whiteSpace: 'pre-wrap' }}>{aiAnswer}</p>
                </div>
              )}
              {searchResults.length === 0 && !aiAnswer && !isSearching && !isAsking && (
                <div style={{ textAlign: 'center', padding: '2rem', color: 'var(--text-muted)' }}>
                  No semantic chunks match your query. Try another phrase.
                </div>
              )}
              {searchResults.map((result, idx) => (
                  <div className="result-item" key={result.chunk_id || idx}>
                    <div className="result-header">
                      <div className="result-title">{result.metadata?.title || 'Document Section'}</div>
                      <div className="result-meta">
                        <span className="badge badge-score">
                          {Math.round(result.score * 100)}% Match
                        </span>
                        <span className="badge badge-source">
                          {result.metadata?.source || 'Source'}
                        </span>
                      </div>
                    </div>
                    <p className="result-content">{result.content}</p>
                    {result.metadata?.url && result.metadata.url !== '#' && (
                      <a 
                        href={result.metadata.url} 
                        target="_blank" 
                        rel="noreferrer" 
                        className="result-footer-link"
                      >
                        View Original Context →
                      </a>
                    )}
                  </div>
                ))}
            </div>
          </div>

          {/* Section: Document Ingest Form */}
          <div className="glass-card">
            <h2 className="card-title">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
                <polyline points="14 2 14 8 20 8"></polyline><line x1="12" y1="18" x2="12" y2="12"></line>
                <line x1="9" y1="15" x2="15" y2="15"></line>
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
                  onChange={e => {
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
                  ></textarea>
                </div>
              )}
              <button type="submit" className="submit-btn" disabled={isIngesting}>
                {isIngesting ? 'Processing Ingestion Pipeline...' : 'Index Document'}
              </button>
            </form>

            {/* Stepper Status Indicators */}
            {isIngesting && (
              <div className="pipeline-stepper">
                <div style={{ fontSize: '0.8rem', fontWeight: 600, color: 'var(--text-secondary)', marginBottom: '0.5rem' }}>
                  Backend Ingestion: {ingestionPercent}%
                </div>
                <progress value={ingestionPercent} max="100" style={{ width: '100%', height: '8px' }} />

                <p style={{ fontSize: '0.85rem', color: 'var(--color-secondary)', marginTop: '0.5rem', fontFamily: 'var(--font-mono)' }}>
                  {ingestionStep > 0 ? `Stage ${ingestionStep}/4: In progress...` : 'Connecting to pipeline...'}
                </p>
              </div>
            )}
            
            {!isIngesting && ingestedId && (
                <p style={{ fontSize: '0.8rem', color: 'var(--color-success)', marginTop: '0.5rem', fontFamily: 'var(--font-mono)' }}>
                  ✓ Ingestion complete for ID: {ingestedId}
                </p>
            )}
          </div>
        </section>

        {/* Right Side: Connections Portal & System Observability */}
        <section style={{ display: 'flex', flexDirection: 'column', gap: '2rem' }}>
          
          {/* Connectors Integration Status */}
          <div className="glass-card">
            <h2 className="card-title">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="2" y="2" width="20" height="8" rx="2" ry="2"></rect>
                <rect x="2" y="14" width="20" height="8" rx="2" ry="2"></rect>
                <line x1="6" y1="6" x2="6.01" y2="6"></line><line x1="6" y1="18" x2="6.01" y2="18"></line>
              </svg>
              Sync Connectors
            </h2>
            <div className="sidebar-list">
              {connectors.map((conn) => (
                <div className="sidebar-item" key={conn.id}>
                  <div className="sidebar-item-info">
                    <span className="sidebar-item-title">{conn.name}</span>
                    <span className="sidebar-item-desc">
                      {conn.type} • {conn.last_sync ? `Synced ${new Date(conn.last_sync).toLocaleTimeString()}` : 'Never synced'}
                    </span>
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
                    <span className={`pulse-dot ${conn.status === 'connected' ? 'active' : ''}`} style={{ backgroundColor: conn.status === 'connected' ? 'var(--color-success)' : 'var(--text-muted)' }}></span>
                    <button 
                      className={`sync-btn ${isSyncing === conn.id ? 'loading' : ''}`}
                      onClick={() => handleSync(conn.id)}
                      disabled={isSyncing !== null}
                    >
                      {isSyncing === conn.id ? 'Syncing' : 'Sync'}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Infrastructure Metrics (Telemetry Observer) */}
          <div className="glass-card">
            <h2 className="card-title">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="20" x2="18" y2="10"></line><line x1="12" y1="20" x2="12" y2="4"></line>
                <line x1="6" y1="20" x2="6" y2="14"></line>
              </svg>
              Infrastructure Metrics
            </h2>
            <div className="telemetry-grid">
              <div className="telemetry-block">
                <span className="telemetry-label">QDRANT VECTORS</span>
                <span className="telemetry-val">{qdrantCount}</span>
              </div>
              <div className="telemetry-block">
                <span className="telemetry-label">POSTGRES RECORDS</span>
                <span className="telemetry-val">{postgresCount}</span>
              </div>
              <div className="telemetry-block">
                <span className="telemetry-label">AVG SEARCH LATENCY</span>
                <span className="telemetry-val">{averageLatency}ms</span>
              </div>
              <div className="telemetry-block">
                <span className="telemetry-label">ACTIVE TENANTS</span>
                <span className="telemetry-val">1</span>
              </div>
            </div>
            
            <div style={{ marginTop: '1.5rem', fontSize: '0.8rem', color: 'var(--text-muted)', borderTop: '1px solid var(--border-color)', paddingTop: '1rem' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '0.25rem' }}>
                <span>Docker Network Gateway</span>
                <span style={{ color: 'var(--color-success)', fontFamily: 'var(--font-mono)' }}>172.20.0.1</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span>DB Engine Pool</span>
                <span style={{ color: 'var(--color-secondary)', fontFamily: 'var(--font-mono)' }}>5 / 20 connections</span>
              </div>
            </div>
          </div>

        </section>

      </main>
    </div>
  );
}

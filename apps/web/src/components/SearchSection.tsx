'use client';

import { useState, useCallback } from 'react';
import type { SearchResult } from '@/lib/types';
import { searchDocuments, askQuestion } from '@/lib/api';
import { SearchResultItem } from './SearchResultItem';
import { SearchSkeleton } from './SearchSkeleton';

export function SearchSection() {
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [searchLatency, setSearchLatency] = useState<number | null>(null);
  const [isAsking, setIsAsking] = useState(false);
  const [aiAnswer, setAiAnswer] = useState<string | null>(null);

  const handleSearch = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (!searchQuery.trim()) return;

    setIsSearching(true);
    setAiAnswer(null);
    setSearchError(null);
    const startTime = performance.now();

    try {
      const data = await searchDocuments(searchQuery);
      setSearchResults(data.results);
      setSearchLatency(data.latency_ms);
    } catch {
      setSearchError('Failed to connect to search service.');
      setSearchResults([]);
    } finally {
      setIsSearching(false);
    }
  }, [searchQuery]);

  const handleAsk = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (!searchQuery.trim()) return;

    setIsAsking(true);
    setSearchResults([]);
    setSearchError(null);
    setAiAnswer('');

    try {
      await askQuestion(searchQuery, 'default', (full) => {
        setAiAnswer(full);
      });
    } catch {
      setAiAnswer('Error: Cannot connect to Answer Engine.');
    } finally {
      setIsAsking(false);
    }
  }, [searchQuery]);

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
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        Hybrid Vector Search
      </h2>

      <form onSubmit={handleSearch} className="search-container">
        <div className="search-input-wrapper">
          <input
            type="text"
            className="search-input"
            placeholder="Ask anything from your synced connections..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          <button
            type="submit"
            className="search-btn"
            disabled={isSearching || isAsking}
          >
            {isSearching ? 'Searching...' : 'Search'}
          </button>
          <button
            type="button"
            className="search-btn"
            style={{ backgroundColor: 'var(--color-secondary)' }}
            onClick={handleAsk}
            disabled={isAsking || isSearching}
          >
            {isAsking ? 'Asking...' : 'Ask AI'}
          </button>
        </div>
        {searchLatency !== null && (
          <p
            style={{
              fontSize: '0.75rem',
              color: 'var(--text-muted)',
              fontFamily: 'var(--font-mono)',
            }}
          >
            Multi-stage search completed in{' '}
            <span style={{ color: 'var(--color-secondary)' }}>
              {searchLatency}ms
            </span>
          </p>
        )}
      </form>

      <div className="results-wrapper">
        {aiAnswer !== null && (
          <div
            className="glass-card"
            style={{
              padding: '1rem',
              marginBottom: '1rem',
              border: '1px solid var(--color-secondary)',
            }}
          >
            <h3
              style={{
                margin: '0 0 0.5rem 0',
                color: 'var(--color-secondary)',
              }}
            >
              AI Answer
            </h3>
            <p style={{ whiteSpace: 'pre-wrap' }}>{aiAnswer}</p>
          </div>
        )}

        {isSearching && <SearchSkeleton />}

        {searchError && (
          <div
            className="glass-card"
            style={{
              padding: '1rem',
              border: '1px solid var(--color-error)',
              backgroundColor: 'rgba(239, 68, 68, 0.05)',
            }}
          >
            <p style={{ color: 'var(--color-error)', fontSize: '0.9rem' }}>
              {searchError}
            </p>
          </div>
        )}

        {!isSearching &&
          !searchError &&
          searchResults.length === 0 &&
          !aiAnswer && (
            <div
              style={{
                textAlign: 'center',
                padding: '2rem',
                color: 'var(--text-muted)',
              }}
            >
              No semantic chunks match your query. Try another phrase.
            </div>
          )}

        {searchResults.map((result) => (
          <SearchResultItem key={result.chunk_id} result={result} />
        ))}
      </div>
    </div>
  );
}

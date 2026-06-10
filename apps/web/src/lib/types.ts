export interface SearchResult {
  chunk_id: string;
  document_id: string;
  content: string;
  score: number;
  metadata: Record<string, unknown>;
}

export interface Connector {
  id: string;
  type: string;
  name: string;
  status: string;
  last_sync: string | null;
}

export interface DocumentItem {
  id: string;
  tenant_id: string;
  title: string;
  content: string;
  file_extension: string | null;
  file_path: string | null;
  status: string;
  metadata: Record<string, unknown>;
  created_at: string;
  completed_at: string | null;
}

export interface IngestionStatus {
  stage: number;
  percent: number;
  message: string;
  status: string;
}

export interface SearchResponse {
  query: string;
  results: SearchResult[];
  latency_ms: number;
  error?: string;
}

export interface DocumentListResponse {
  documents: DocumentItem[];
  total: number;
}

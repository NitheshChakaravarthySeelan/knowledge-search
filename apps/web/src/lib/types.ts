export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string;
}

export interface SessionSummary {
  id: string;
  preview: string;
  message_count: number;
  last_timestamp: string;
}

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

export interface SearchConfig {
  rrf_k?: number;
  dense_weight?: number;
  sparse_weight?: number;
  entity_weight?: number;
  graph_weight?: number;
}

export interface AskRequest {
  question: string;
  session_id?: string;
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

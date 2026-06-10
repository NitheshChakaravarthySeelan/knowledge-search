import type { SearchResponse, DocumentItem, DocumentListResponse, IngestionStatus } from './types';

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8000';

export async function searchDocuments(
  query: string,
  limit = 5,
  tenantId = 'default',
): Promise<SearchResponse> {
  const response = await fetch(`${API_BASE}/api/search`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, limit, tenant_id: tenantId }),
  });
  if (!response.ok) throw new Error(`Search failed: ${response.status}`);
  return response.json();
}

export async function askQuestion(
  question: string,
  tenantId = 'default',
  onChunk: (text: string) => void,
): Promise<void> {
  const url = `${API_BASE}/api/ask`;
  console.log('[askQuestion] calling', url);

  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ question, tenant_id: tenantId }),
  });

  console.log('[askQuestion] response status', response.status, response.ok);

  if (!response.ok) {
    const errText = await response.text().catch(() => 'unknown');
    throw new Error(`Ask failed: ${response.status} ${errText}`);
  }

  if (!response.body) throw new Error('No response body');

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let fullAnswer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    fullAnswer += decoder.decode(value, { stream: true });
    onChunk(fullAnswer);
  }
}

export async function ingestDocument(
  title: string,
  content: string,
  fileExtension?: string,
): Promise<{ success: boolean; document_id: string }> {
  const response = await fetch(`${API_BASE}/api/documents`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title, content, fileExtension }),
  });
  if (!response.ok) throw new Error(`Ingest failed: ${response.status}`);
  return response.json();
}

export async function getIngestionStatus(
  id: string,
): Promise<IngestionStatus> {
  const response = await fetch(`${API_BASE}/api/documents/${id}/status`);
  if (!response.ok) throw new Error(`Status check failed: ${response.status}`);
  return response.json();
}

export async function listDocuments(): Promise<DocumentListResponse> {
  const response = await fetch(`${API_BASE}/api/documents`);
  if (!response.ok) throw new Error(`List failed: ${response.status}`);
  return response.json();
}

export async function getDocument(id: string): Promise<DocumentItem> {
  const response = await fetch(`${API_BASE}/api/documents/${id}`);
  if (!response.ok) throw new Error(`Get document failed: ${response.status}`);
  return response.json();
}

export async function updateDocument(
  id: string,
  data: { title?: string; content?: string; metadata?: Record<string, unknown> },
): Promise<DocumentItem> {
  const response = await fetch(`${API_BASE}/api/documents/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  if (!response.ok) throw new Error(`Update failed: ${response.status}`);
  return response.json();
}

export async function deleteDocument(id: string): Promise<void> {
  const response = await fetch(`${API_BASE}/api/documents/${id}`, {
    method: 'DELETE',
  });
  if (!response.ok) throw new Error(`Delete failed: ${response.status}`);
}

export async function syncConnector(id: string): Promise<void> {
  const response = await fetch(`${API_BASE}/api/connectors/${id}/sync`, {
    method: 'POST',
  });
  if (!response.ok) throw new Error(`Sync failed: ${response.status}`);
}

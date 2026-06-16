'use client';

import { useState, useCallback } from 'react';
import { ChatSidebar } from '@/components/ChatSidebar';
import { ChatThread } from '@/components/ChatThread';
import { IngestionModal } from '@/components/IngestionModal';
import { FabButton } from '@/components/FabButton';

export default function Home() {
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [sessionsChanged, setSessionsChanged] = useState(0);
  const [modalOpen, setModalOpen] = useState(false);

  const handleSelectSession = useCallback((id: string) => {
    setActiveSessionId(id);
  }, []);

  const handleNewSession = useCallback(() => {
    setActiveSessionId(null);
  }, []);

  const handleSessionIdChange = useCallback((id: string) => {
    setActiveSessionId(id);
    setSessionsChanged((prev) => prev + 1);
  }, []);

  const handleModalClose = useCallback(() => {
    setModalOpen(false);
    setSessionsChanged((prev) => prev + 1);
  }, []);

  return (
    <div style={{ display: 'flex', height: '100vh', overflow: 'hidden' }}>
      <ChatSidebar
        activeSessionId={activeSessionId}
        onSelectSession={handleSelectSession}
        onNewSession={handleNewSession}
        refreshTrigger={sessionsChanged}
      />

      <ChatThread
        sessionId={activeSessionId}
        onSessionIdChange={handleSessionIdChange}
      />

      <FabButton onClick={() => setModalOpen(true)} />

      <IngestionModal open={modalOpen} onClose={handleModalClose} />
    </div>
  );
}

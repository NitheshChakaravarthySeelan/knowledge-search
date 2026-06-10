'use client';

import { Header } from '@/components/Header';
import { SearchSection } from '@/components/SearchSection';
import { IngestionForm } from '@/components/IngestionForm';
import { ConnectorsSection } from '@/components/ConnectorsSection';
import { MetricsSection } from '@/components/MetricsSection';
import { DocumentManager } from '@/components/DocumentManager';

export default function Home() {
  return (
    <div className="dashboard-wrapper">
      <Header />

      <main className="grid-container">
        <section
          style={{ display: 'flex', flexDirection: 'column', gap: '2rem' }}
        >
          <SearchSection />
          <IngestionForm />
          <DocumentManager />
        </section>

        <section
          style={{ display: 'flex', flexDirection: 'column', gap: '2rem' }}
        >
          <ConnectorsSection />
          <MetricsSection />
        </section>
      </main>
    </div>
  );
}

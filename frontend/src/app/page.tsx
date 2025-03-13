'use client';

import { useEffect, useState } from 'react';
import { connectDb } from '@/lib/connectDb';
import type PouchDB from 'pouchdb';

interface Doc {
  _id: string;
  _rev: string;
  [key: string]: any;
}

export default function Home() {
  const [docs, setDocs] = useState<Doc[]>([]);
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function loadData() {
      try {
        const db = await connectDb();
        const result = await db.allDocs({
          include_docs: true,
          attachments: true
        });

        setDocs(result.rows.map(row => row.doc!));
      } catch (err) {
        console.error('Failed to load docs:', err);
        setError(err instanceof Error ? err.message : 'Failed to load documents');
      } finally {
        setLoading(false);
      }
    }

    loadData();
  }, []);

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error}</div>;

  return (
    <main className="p-4">
      <h1 className="text-2xl font-bold mb-4">Database Contents</h1>
      <div className="space-y-4">
        {docs.map(doc => (
          <div key={doc._id} className="p-4 border rounded">
            <h2 className="font-bold">{doc._id}</h2>
            <pre className="mt-2 bg-gray-100 p-2 rounded">
              {JSON.stringify(doc, null, 2)}
            </pre>
          </div>
        ))}
      </div>
    </main>
  );
}

'use client';

import { useState, useEffect } from 'react';
import { connectDb } from '@/lib/connectDb_old';

interface DayviewPanelProps {
  date: string;
}

export function DayviewPanel({ date }: DayviewPanelProps) {
  const [svgContent, setSvgContent] = useState<string>('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();

  useEffect(() => {
    async function fetchDayview() {
      try {
        setLoading(true);
        await connectDb(); // We don't need the DB instance in the client

        // Calculate panel dimensions
        const width = Math.min(window.innerWidth * 0.45, 800);
        const height = Math.min(window.innerHeight * 0.8, 600);

        const response = await fetch('/api/dayview', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            date,
            width,
            height,
            dayHeight: 100,
            timeAxisHeight: 20
          })
        });

        if (!response.ok) throw new Error('Failed to fetch dayview');
        const svg = await response.text();
        setSvgContent(svg);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load dayview');
      } finally {
        setLoading(false);
      }
    }

    fetchDayview();
  }, [date]);

  return (
    <div className="bg-black rounded-lg p-4">
      <h2 className="text-xl font-bold mb-4">Day View</h2>
      {loading ? (
        <div className="flex items-center justify-center h-[400px]">Loading...</div>
      ) : error ? (
        <div className="text-red-500">{error}</div>
      ) : (
        <div
          className="overflow-auto"
          dangerouslySetInnerHTML={{ __html: svgContent }}
        />
      )}
    </div>
  );
}
'use client';

import { useState } from 'react';
import { DayviewPanel } from './panels/DayviewPanel';
import { TailPanel } from './panels/TailPanel';
import { DateTime } from 'luxon';

export function Dashboard() {
  const [date, setDate] = useState(DateTime.now().toISODate());

  return (
    <div className="min-h-screen bg-gray-900 text-white">
      <div className="container mx-auto p-4">
        <div className="flex justify-between items-center mb-4">
          <h1 className="text-2xl font-bold">Dashboard</h1>
          <input
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
            className="bg-gray-800 text-white px-4 py-2 rounded"
          />
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <DayviewPanel date={date} />
          <TailPanel date={date} />
        </div>
      </div>
    </div>
  );
}
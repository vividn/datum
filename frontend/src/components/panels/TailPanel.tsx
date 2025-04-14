"use client";

import { useState, useEffect } from "react";
import { connectDb } from "../../../../src/auth/connectDb";
import { getContrastTextColor } from "../../../../src/utils/colorUtils";
import { getFieldColor, getStateColor } from "../../../../src/field/fieldColor";
import { DatumDocument } from "../../../../src/documentControl/DatumDocument";

interface TailEntry {
  _id: string;
  field: string;
  state: string;
  timestamp: string;
}

interface TailPanelProps {
  date: string;
}

export function TailPanel({ date }: TailPanelProps) {
  const [entries, setEntries] = useState<TailEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();

  useEffect(() => {
    async function fetchTail() {
      try {
        setLoading(true);
        const db = connectDb({});

        // Query the database directly
        const result = await db.query("datum/by_timestamp", {
          descending: true,
          limit: 20,
          startkey: date,
          include_docs: true,
        });

        const data = result.rows.map((row) => ({
          _id: row.id,
          field: (row.doc as DatumDocument).data.field,
          state: (row.doc as DatumDocument).data.state,
          timestamp: (row.doc as DatumDocument).data.occurTime,
        }));

        setEntries(data);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to load tail data",
        );
      } finally {
        setLoading(false);
      }
    }

    fetchTail();
  }, [date]);

  return (
    <div className="bg-black rounded-lg p-4">
      <h2 className="text-xl font-bold mb-4">Recent Events</h2>
      {loading ? (
        <div className="flex items-center justify-center h-[400px]">
          Loading...
        </div>
      ) : error ? (
        <div className="text-red-500">{error}</div>
      ) : (
        <div className="space-y-2 max-h-[600px] overflow-auto">
          {entries.map((entry) => {
            const fieldColor = getFieldColor(entry.field);
            const stateColor = getStateColor(entry.state);
            const textColor = getContrastTextColor(fieldColor);

            return (
              <div
                key={entry._id}
                className="p-2 rounded"
                style={{ backgroundColor: fieldColor }}
              >
                <div className="flex justify-between items-center">
                  <span style={{ color: textColor }}>{entry.field}</span>
                  <span
                    className="px-2 py-1 rounded"
                    style={{
                      backgroundColor: stateColor,
                      color: getContrastTextColor(stateColor),
                    }}
                  >
                    {entry.state}
                  </span>
                </div>
                <div
                  className="text-sm opacity-75"
                  style={{ color: textColor }}
                >
                  {new Date(entry.timestamp).toLocaleString()}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}


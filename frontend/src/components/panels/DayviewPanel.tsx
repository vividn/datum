"use client";

import { useState, useEffect } from "react";
import { dayview } from "../../../../src/dayview/dayview";
import { dayviewCmd } from "../../../../src/commands/dayviewCmd";

interface DayviewPanelProps {
  date: string;
}

export function DayviewPanel({ date }: DayviewPanelProps) {
  const [svgContent, setSvgContent] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();

  useEffect(() => {
    async function fetchDayview() {
      try {
        setLoading(true);

        // Calculate panel dimensions
        const width = Math.min(window.innerWidth * 0.45, 800);
        const height = Math.min(window.innerHeight * 0.8, 600);

        const dayviewSvg = await dayviewCmd({
          endDate: date,
          width,
          height,
          dayHeight: 100,
          timeAxisHeight: 20,
        });

        setSvgContent(dayviewSvg);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load dayview");
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
        <div className="flex items-center justify-center h-[400px]">
          Loading...
        </div>
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


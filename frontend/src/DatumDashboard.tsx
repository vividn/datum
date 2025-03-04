import React, { useState, useEffect } from 'react';
import { DateTime } from 'luxon'; // Add for better date handling

// Use fetch or a proper API method instead of window.fs
const fetchDayviewData = async (startDate: string, width: number, endDate?: string) => {
  try {
    // Build the URL path with endDate as part of the path, not query param
    const basePath = `http://localhost:3001/api/dayview/${startDate}`;
    const url = new URL(endDate ? `${basePath}/${endDate}` : basePath);

    // Add width as query parameter
    url.searchParams.append('width', width.toString());

    console.log(url.toString());
    const response = await fetch(url.toString());
    if (!response.ok) {
      throw new Error('Failed to fetch dayview data');
    }

    return await response.text();
  } catch (error) {
    console.error('Error fetching dayview:', error);
    return null;
  }
};

interface DatumStats {
  totalHours: number;
  categories: Record<string, number>;
  mostActiveHour: string;
}

const DatumDashboard: React.FC = () => {
  const today = DateTime.now();
  const tomorrow = today.plus({ days: 1 });

  const [dayviewData, setDayviewData] = useState<string | null>(null);
  const [startDate, setStartDate] = useState(today.toISODate());
  const [endDate, setEndDate] = useState(tomorrow.toISODate());
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<'day' | 'week' | 'month'>('day');
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [stats, setStats] = useState<DatumStats | null>(null);

  // Quick select buttons for common ranges
  const setDateRange = (days: number) => {
    const end = DateTime.now();
    const start = end.minus({ days: days - 1 });

    if (days === 1) {
      // For single day, ensure start and end are the same
      setStartDate(end.toISODate());
      setEndDate(end.plus({ days: 1 }).toISODate());
    } else {
      // For multiple days, use the range
      setStartDate(start.toISODate());
      setEndDate(end.plus({ days: 1 }).toISODate());
    }
  };

  const loadDayview = async () => {
    setError(null);
    setIsLoading(true);
    try {
      const width = Math.max(window.innerWidth - 32, 320);

      // Only pass endDate if it's different from startDate + 1 day
      const startDateTime = DateTime.fromISO(startDate);
      const endDateTime = DateTime.fromISO(endDate);
      const isNextDay = startDateTime.plus({ days: 1 }).toISODate() === endDate;

      const svgContent = await fetchDayviewData(
        startDate,
        width,
        isNextDay ? undefined : endDate
      );

      if (!svgContent) {
        throw new Error('No data received');
      }
      setDayviewData(svgContent);
    } catch (error) {
      console.error('Failed to load dayview:', error);
      setError(error instanceof Error ? error.message : 'Failed to load data');
      setDayviewData(null);
    } finally {
      setIsLoading(false);
    }
  };

  // const loadStats = async () => {
  //   try {
  //     const endDateParam = startDate !== endDate ? `?endDate=${endDate}` : '';
  //     const response = await fetch(`http://localhost:3001/api/stats/${startDate}${endDateParam}`);
  //     if (!response.ok) throw new Error('Failed to fetch stats');
  //     const data = await response.json();
  //     setStats(data);
  //   } catch (error) {
  //     console.error('Failed to load stats:', error);
  //   }
  // };

  // Auto-refresh timer
  useEffect(() => {
    if (!autoRefresh) return;

    const timer = setInterval(() => {
      loadDayview();
    }, 60000); // Refresh every minute

    return () => clearInterval(timer);
  }, [autoRefresh, startDate, endDate]);

  useEffect(() => {
    const handleResize = () => {
      loadDayview();
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [startDate, endDate]); // Re-run when dates change

  useEffect(() => {
    loadDayview();
  }, [startDate, endDate]);

  // useEffect(() => {
  //   loadStats();
  // }, [startDate, endDate]);

  return (
    <div className="p-4 max-w-6xl mx-auto">
      <div className="flex justify-between items-center mb-6">
        <h1 className="text-2xl font-bold">Datum Dashboard</h1>

        {/* View mode selector */}
        <div className="flex items-center gap-4">
          <div className="flex bg-gray-100 rounded-lg p-1">
            {(['day', 'week', 'month'] as const).map(mode => (
              <button
                key={mode}
                onClick={() => {
                  setViewMode(mode);
                  if (mode === 'day') setDateRange(1);
                  if (mode === 'week') setDateRange(7);
                  if (mode === 'month') setDateRange(30);
                }}
                className={`px-4 py-2 rounded-md ${
                  viewMode === mode
                    ? 'bg-white shadow-sm'
                    : 'hover:bg-gray-200'
                }`}
              >
                {mode.charAt(0).toUpperCase() + mode.slice(1)}
              </button>
            ))}
          </div>

          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="auto-refresh"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="rounded"
            />
            <label htmlFor="auto-refresh">Auto-refresh</label>
          </div>
        </div>
      </div>

      <div className="bg-white shadow-sm rounded-lg p-4 mb-4">
        {/* Quick select buttons */}
        <div className="flex gap-2 mb-4">
          <button
            onClick={() => {
              const today = DateTime.now();
              setStartDate(today.toISODate());
              setEndDate(today.toISODate()); // Set to same day instead of tomorrow
            }}
            className="px-3 py-1 bg-blue-100 text-blue-700 rounded hover:bg-blue-200"
          >
            Today
          </button>
          <button
            onClick={() => setDateRange(3)}
            className="px-3 py-1 bg-gray-200 rounded hover:bg-gray-300"
          >
            Last 3 Days
          </button>
          <button
            onClick={() => setDateRange(7)}
            className="px-3 py-1 bg-gray-200 rounded hover:bg-gray-300"
          >
            Last Week
          </button>
          <button
            onClick={() => setDateRange(30)}
            className="px-3 py-1 bg-gray-200 rounded hover:bg-gray-300"
          >
            Last Month
          </button>
          <button
            onClick={() => {
              const start = DateTime.now().startOf('week');
              setStartDate(start.toISODate());
              setEndDate(start.plus({ days: 6 }).toISODate());
            }}
            className="px-3 py-1 bg-blue-100 text-blue-700 rounded hover:bg-blue-200"
          >
            This Week
          </button>
        </div>

        {/* Date range inputs */}
        <div className="flex items-center gap-4">
          <div className="flex items-center">
            <label htmlFor="start-date" className="mr-2">From:</label>
            <input
              id="start-date"
              type="date"
              value={startDate}
              onChange={(e) => setStartDate(e.target.value)}
              className="border p-2"
            />
          </div>

          <div className="flex items-center">
            <label htmlFor="end-date" className="mr-2">To:</label>
            <input
              id="end-date"
              type="date"
              value={endDate}
              min={startDate}
              onChange={(e) => setEndDate(e.target.value)}
              className="border p-2"
            />
          </div>

          <button
            onClick={loadDayview}
            className="ml-2 p-2 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:bg-blue-300"
            disabled={isLoading}
          >
            {isLoading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && (
        <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">
          {error}
        </div>
      )}

      {isLoading ? (
        <div className="flex items-center justify-center h-64 bg-gray-50 rounded-lg">
          <div className="animate-spin rounded-full h-12 w-12 border-4 border-blue-500 border-t-transparent"></div>
        </div>
      ) : dayviewData ? (
        <div className="bg-white shadow-sm rounded-lg overflow-hidden">
          <div
            dangerouslySetInnerHTML={{ __html: dayviewData }}
            className="overflow-auto"
          />
        </div>
      ) : (
        <div className="text-center py-12 bg-gray-50 rounded-lg">
          <p className="text-gray-500">No data available for selected dates</p>
        </div>
      )}

      {/* Statistics Panel
      <div className="mt-6 grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="bg-white p-4 rounded-lg shadow-sm">
          <h3 className="text-lg font-semibold mb-2">Total Hours</h3>
          {stats ? (
            <div className="text-3xl font-bold text-blue-600">
              {stats.totalHours}
              <span className="text-sm text-gray-500 ml-1">hrs</span>
            </div>
          ) : (
            <div className="animate-pulse h-8 bg-gray-200 rounded w-24" />
          )}
        </div>

        <div className="bg-white p-4 rounded-lg shadow-sm">
          <h3 className="text-lg font-semibold mb-2">Most Active Times</h3>
          {stats ? (
            <div>
              <div className="text-xl font-medium text-gray-700">
                {stats.mostActiveHour}
              </div>
              <div className="text-sm text-gray-500">Peak activity</div>
            </div>
          ) : (
            <div className="animate-pulse h-8 bg-gray-200 rounded w-32" />
          )}
        </div>

        <div className="bg-white p-4 rounded-lg shadow-sm">
          <h3 className="text-lg font-semibold mb-2">Top Categories</h3>
          {stats ? (
            <div className="space-y-2">
              {Object.entries(stats.categories)
                .sort(([, a], [, b]) => b - a)
                .slice(0, 3)
                .map(([category, minutes]) => (
                  <div key={category} className="flex justify-between items-center">
                    <span className="text-gray-700">{category}</span>
                    <span className="text-gray-500">
                      {Math.round(minutes / 60 * 10) / 10}hrs
                    </span>
                  </div>
                ))}
            </div>
          ) : (
            <div className="space-y-2">
              {[1, 2, 3].map(i => (
                <div key={i} className="animate-pulse h-6 bg-gray-200 rounded" />
              ))}
            </div>
          )}
        </div>
      </div> */}
    </div>
  );
};

export default DatumDashboard;
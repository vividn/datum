import React, { useState, useEffect, useRef } from 'react';
import { DateTime } from 'luxon'; // Add for better date handling
import { getStateColor, getFieldColor } from '@vividn/datum/src/field/fieldColor';
import { getContrastTextColor } from '@vividn/datum/src/utils/colorUtils';

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

interface DatumEntry {
  id: string;
  timestamp: string;
  text: string;
  category?: string;
  type?: string;
}

const DatumDashboard: React.FC = () => {
  const today = DateTime.now();
  const tomorrow = today.plus({ days: 1 });

  const [dayviewData, setDayviewData] = useState<string | null>(null);
  const [startDate, setStartDate] = useState(today.toISODate());
  const [endDate, setEndDate] = useState(tomorrow.toISODate());
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<'day' | 'week' | 'month' | 'tail'>('day');
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [stats, setStats] = useState<DatumStats | null>(null);

  // New state for datum tail feature
  const [tailEntries, setTailEntries] = useState<DatumEntry[]>([]);
  const [tailLoading, setTailLoading] = useState(false);
  const [tailError, setTailError] = useState<string | null>(null);
  const [tailLimit, setTailLimit] = useState(20);
  const [tailAutoScroll, setTailAutoScroll] = useState(true);
  const tailEndRef = useRef<HTMLDivElement>(null);

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
    if (viewMode === 'tail') return; // Skip loading dayview when in tail mode

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

  // New state for tail settings
  const [tailField, setTailField] = useState<string>('');

  // New function to fetch tail data
  const fetchTailData = async () => {
    setTailLoading(true);
    setTailError(null);
    try {
      // Build query parameters
      const params = new URLSearchParams({
        limit: tailLimit.toString(),
      });

      if (tailField.trim()) {
        params.append('field', tailField);
      }

      const response = await fetch(`http://localhost:3001/api/tail?${params.toString()}`);
      if (!response.ok) {
        throw new Error('Failed to fetch tail data');
      }
      const data = await response.json();
      setTailEntries(data);
    } catch (error) {
      console.error('Error fetching tail data:', error);
      setTailError(error instanceof Error ? error.message : 'Failed to load tail data');
    } finally {
      setTailLoading(false);
    }
  };

  // Scroll to bottom of tail entries when new entries come in
  useEffect(() => {
    if (tailAutoScroll && tailEndRef.current && viewMode === 'tail') {
      tailEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [tailEntries, tailAutoScroll, viewMode]);

  // Auto-refresh timer for dayview
  useEffect(() => {
    if (!autoRefresh) return;

    const timer = setInterval(() => {
      if (viewMode === 'tail') {
        fetchTailData();
      } else {
        loadDayview();
      }
    }, 60000); // Refresh every minute

    return () => clearInterval(timer);
  }, [autoRefresh, startDate, endDate, viewMode, tailLimit]);

  useEffect(() => {
    const handleResize = () => {
      if (viewMode !== 'tail') {
        loadDayview();
      }
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [startDate, endDate, viewMode]); // Re-run when dates or viewMode change

  useEffect(() => {
    if (viewMode === 'tail') {
      fetchTailData();
    } else {
      loadDayview();
    }
  }, [startDate, endDate, viewMode, tailLimit]);

  // Format timestamp for display in tail view
  const formatTimestamp = (timestamp: string) => {
    try {
      // Ensure timestamp is in ISO format by replacing space with 'T'
      const isoTimestamp = timestamp.replace(' ', 'T');
      return DateTime.fromISO(isoTimestamp).toFormat('yyyy-MM-dd HH:mm:ss');
    } catch (e) {
      console.error('Error parsing timestamp:', timestamp, e);
      return timestamp;
    }
  };

  // Function to determine the color based on entry category or type
  const getCategoryColor = (entry: DatumEntry) => {
    const category = entry.category?.toLowerCase() || entry.type?.toLowerCase();

    if (!category) return 'bg-gray-100';

    if (category.includes('work')) return 'bg-blue-100';
    if (category.includes('personal')) return 'bg-green-100';
    if (category.includes('meeting')) return 'bg-purple-100';
    if (category.includes('break') || category.includes('rest')) return 'bg-yellow-100';
    if (category.includes('exercise')) return 'bg-orange-100';
    if (category.includes('learning')) return 'bg-indigo-100';
    if (category.includes('error')) return 'bg-red-100';

    // Hash the category string to get a consistent color
    const hash = category.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
    const colors = [
      'bg-blue-100', 'bg-green-100', 'bg-yellow-100',
      'bg-purple-100', 'bg-pink-100', 'bg-indigo-100'
    ];
    return colors[hash % colors.length];
  };

  const TailEntry = ({ entry }: { entry: DatumEntry }) => {
    const timestamp = formatTimestamp(entry.timestamp);

    // Get colors for the field and state
    const fieldColor = getFieldColor(entry.category);
    const stateColor = getStateColor({
      state: entry.type,
      field: entry.category
    });

    // Get contrasting text colors
    const fieldTextColor = getContrastTextColor(fieldColor);
    const stateTextColor = getContrastTextColor(stateColor);

    return (
      <div style={{
        fontFamily: 'monospace',
        whiteSpace: 'pre',
        margin: '2px 0'
      }}>
        {timestamp}
        <span style={{
          backgroundColor: fieldColor,
          color: fieldTextColor,
          padding: '0 4px',
          marginLeft: '2px',
          marginRight: '2px'
        }}>
          {entry.category?.padEnd(12)}
        </span>
        <span style={{
          backgroundColor: stateColor,
          color: stateTextColor,
          padding: '0 4px'
        }}>
          {entry.type?.padEnd(10)}
        </span>
        <span style={{ marginLeft: '4px', color: '#666' }}>
          {entry.id?.padEnd(6)}
        </span>
      </div>
    );
  };

  return (
    <div className="p-4 max-w-6xl mx-auto">
      <div className="flex justify-between items-center mb-6">
        <h1 className="text-2xl font-bold">Datum Dashboard</h1>

        {/* View mode selector */}
        <div className="flex items-center gap-4">
          <div className="flex bg-gray-100 rounded-lg p-1">
            {(['day', 'week', 'month', 'tail'] as const).map(mode => (
              <button
                key={mode}
                onClick={() => {
                  setViewMode(mode);
                  if (mode === 'day') setDateRange(1);
                  if (mode === 'week') setDateRange(7);
                  if (mode === 'month') setDateRange(30);
                  if (mode === 'tail') fetchTailData();
                }}
                className={`px-4 py-2 rounded-md ${
                  viewMode === mode
                    ? 'bg-white shadow-sm'
                    : 'hover:bg-gray-200'
                }`}
              >
                {mode === 'tail' ? 'Tail' : mode.charAt(0).toUpperCase() + mode.slice(1)}
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

      {viewMode !== 'tail' ? (
        // Dayview controls and display
        <>
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
        </>
      ) : (
        // Tail View
        <div className="bg-white shadow-sm rounded-lg p-4">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-semibold">Recent Entries</h2>

                          <div className="flex flex-wrap items-center gap-4">
                <div className="flex items-center gap-2">
                  <label htmlFor="tail-limit" className="text-sm">Limit:</label>
                  <select
                    id="tail-limit"
                    value={tailLimit}
                    onChange={(e) => setTailLimit(Number(e.target.value))}
                    className="border rounded p-1"
                  >
                    <option value={10}>10</option>
                    <option value={20}>20</option>
                    <option value={50}>50</option>
                    <option value={100}>100</option>
                  </select>
                </div>

                <div className="flex items-center gap-2">
                  <label htmlFor="tail-field" className="text-sm">Field:</label>
                  <input
                    type="text"
                    id="tail-field"
                    value={tailField}
                    onChange={(e) => setTailField(e.target.value)}
                    placeholder="Filter by field"
                    className="border rounded p-1 w-28"
                  />
                </div>

                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="tail-autoscroll"
                    checked={tailAutoScroll}
                    onChange={(e) => setTailAutoScroll(e.target.checked)}
                    className="rounded"
                  />
                  <label htmlFor="tail-autoscroll" className="text-sm">Auto-scroll</label>
                </div>

                <button
                  onClick={fetchTailData}
                  className="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:bg-blue-300"
                  disabled={tailLoading}
                >
                  {tailLoading ? 'Loading...' : 'Refresh'}
                </button>
              </div>
          </div>

          {tailError && (
            <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">
              {tailError}
            </div>
          )}

          {tailLoading && tailEntries.length === 0 ? (
            <div className="flex items-center justify-center h-32">
              <div className="animate-spin rounded-full h-8 w-8 border-4 border-blue-500 border-t-transparent"></div>
            </div>
          ) : (
            <div className="overflow-y-auto max-h-screen">
              {tailEntries.length === 0 ? (
                <div className="text-center py-8 text-gray-500">
                  No entries found
                </div>
              ) : (
                <div className="space-y-2">
                  {tailEntries.map((entry, index) => (
                    <TailEntry key={entry.id || `entry-${index}`} entry={entry} />
                  ))}
                  <div ref={tailEndRef} /> {/* Invisible element for auto-scrolling */}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default DatumDashboard;
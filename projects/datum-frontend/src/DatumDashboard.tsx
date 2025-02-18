import React, { useState, useEffect } from 'react';

// Use fetch or a proper API method instead of window.fs
const fetchDayviewData = async (date: string) => {
  try {
    // Updated API endpoint to match backend route
    const response = await fetch(`http://localhost:3001/api/dayview/${date}`);
    if (!response.ok) {
      throw new Error('Failed to fetch dayview data');
    }
    return await response.text();
  } catch (error) {
    console.error('Error fetching dayview:', error);
    return null;
  }
};

const DatumDashboard: React.FC = () => {
  const today = new Date().toISOString().split('T')[0];
  const tomorrow = new Date(Date.now() + 86400000).toISOString().split('T')[0];

  const [dayviewData, setDayviewData] = useState<string | null>(null);
  const [startDate, setStartDate] = useState(today);
  const [endDate, setEndDate] = useState(tomorrow);
  const [isLoading, setIsLoading] = useState(false);

  // Quick select buttons for common ranges
  const setDateRange = (days: number) => {
    const end = new Date();
    const start = new Date();
    start.setDate(start.getDate() - (days - 1));
    setStartDate(start.toISOString().split('T')[0]);
    setEndDate(end.toISOString().split('T')[0]);
  };

  const loadDayview = async () => {
    setIsLoading(true);
    try {
      const width = Math.max(window.innerWidth - 32, 320);
      const endDateParam = startDate !== endDate ? `&endDate=${endDate}` : '';
      const svgContent = await fetchDayviewData(`${startDate}?width=${width}${endDateParam}`);
      setDayviewData(svgContent);
    } catch (error) {
      console.error('Failed to load dayview:', error);
      setDayviewData(null);
    } finally {
      setIsLoading(false);
    }
  };

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

  return (
    <div className="p-4 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold mb-4">Datum Dashboard</h1>

      <div className="mb-4">
        {/* Quick select buttons */}
        <div className="flex gap-2 mb-4">
          <button
            onClick={() => {
              setStartDate(today);
              setEndDate(tomorrow);
            }}
            className="px-3 py-1 bg-gray-200 rounded hover:bg-gray-300"
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

      {isLoading ? (
        <p>Loading dayview...</p>
      ) : dayviewData ? (
        <div
          dangerouslySetInnerHTML={{ __html: dayviewData }}
          className="overflow-auto border"
        />
      ) : (
        <p>No data available for selected dates</p>
      )}
    </div>
  );
};

export default DatumDashboard;
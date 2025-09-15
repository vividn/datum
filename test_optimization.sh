#!/bin/bash

echo "🚀 Testing Dayview Optimization"
echo "================================"

# Create a temporary database for testing
DB_NAME="test_optimization_$(date +%s)"
echo "📊 Using database: $DB_NAME"

echo ""
echo "1️⃣ Starting optimized dayview in background..."
# Start the optimized dayview in watch mode
pnpm dev dayview -n 3 --db "$DB_NAME" -o "/tmp/dayview_optimized.svg" --watch &
DAYVIEW_PID=$!

echo "   Process ID: $DAYVIEW_PID"
echo "   Waiting 3 seconds for initialization..."
sleep 3

echo ""
echo "2️⃣ Adding test data to trigger changes..."

# Add some test data with today's date
TODAY=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
YESTERDAY=$(date -u -d "yesterday" +"%Y-%m-%dT%H:%M:%S.000Z" 2>/dev/null || date -u -v-1d +"%Y-%m-%dT%H:%M:%S.000Z")

echo "   📝 Adding entry for today ($TODAY)..."
pnpm dev entry --db "$DB_NAME" --field "test_today" --value "Testing optimization today" --occur-time "$TODAY" || echo "   ⚠️  Entry command failed (might not be available)"

sleep 2

echo "   📝 Adding entry for yesterday ($YESTERDAY)..."
pnpm dev entry --db "$DB_NAME" --field "test_yesterday" --value "Testing optimization yesterday" --occur-time "$YESTERDAY" || echo "   ⚠️  Entry command failed (might not be available)"

sleep 2

echo "   📝 Updating today's entry..."
pnpm dev entry --db "$DB_NAME" --field "test_today" --value "Updated: Testing optimization today" --occur-time "$TODAY" || echo "   ⚠️  Entry command failed (might not be available)"

echo ""
echo "3️⃣ Letting it run for 10 more seconds to observe changes..."
sleep 10

echo ""
echo "4️⃣ Stopping dayview watch mode..."
kill $DAYVIEW_PID 2>/dev/null
wait $DAYVIEW_PID 2>/dev/null

echo ""
echo "📊 Results:"
echo "   - Check the terminal output above for 'redrawing X changed day(s)' messages"
echo "   - The optimized version should show specific days being redrawn"
echo "   - Output file: /tmp/dayview_optimized.svg"

if [ -f "/tmp/dayview_optimized.svg" ]; then
    echo "   ✅ SVG file created successfully ($(wc -c < /tmp/dayview_optimized.svg) bytes)"
else
    echo "   ❌ SVG file not created"
fi

echo ""
echo "🧹 Cleanup:"
echo "   Database '$DB_NAME' was created for this test"
echo "   You can remove it with: pnpm dev db-admin --db '$DB_NAME' --delete"
echo ""
echo "✨ Test complete!"
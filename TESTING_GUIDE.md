# Testing Dayview Optimization Guide

## Quick Interactive Test

### Step 1: Start Optimized Watch Mode
```bash
# In Terminal 1 - Start the optimized dayview
pnpm dev dayview -n 3 --db test_opt -o dayview_output.svg --watch
```

**Expected output:**
```
watching (optimized mode)
```

### Step 2: Add Data in Another Terminal
```bash
# In Terminal 2 - Add some test data
pnpm dev entry --db test_opt --field mood --value happy
pnpm dev entry --db test_opt --field activity --value coding
```

**Expected output in Terminal 1:**
```
redrawing 1 changed day(s): 2025-07-03
```

### Step 3: Add Data for Different Day
```bash
# Add data for yesterday
pnpm dev entry --db test_opt --field mood --value excited --occur-time "2025-07-02T10:00:00Z"
```

**Expected output in Terminal 1:**
```
redrawing 1 changed day(s): 2025-07-02
```

### Step 4: Update Multiple Days
```bash
# Update today
pnpm dev entry --db test_opt --field mood --value "very happy"

# Update yesterday  
pnpm dev entry --db test_opt --field activity --value "testing" --occur-time "2025-07-02T11:00:00Z"
```

**Expected output in Terminal 1:**
```
redrawing 2 changed day(s): 2025-07-03, 2025-07-02
```

## What to Look For

### ✅ Success Indicators:
1. **"watching (optimized mode)"** - Confirms optimization is active
2. **Specific day redraws** - Messages like "redrawing 1 changed day(s): 2025-07-03"
3. **Multiple day updates** - When multiple days change, it lists them all
4. **Debounced updates** - Changes within 1 second are grouped together
5. **Periodic redraws** - Every 5 minutes: "periodic redraw (no changes detected)"

### ❌ Old Behavior (would show):
- Just "redrawing" without specifying which days
- Full redraws even for single day changes

## Performance Comparison

### Test Large Dataset:
```bash
# Create a week view with more days to see performance difference
pnpm dev dayview -n 7 --db test_opt -o dayview_week.svg --watch
```

With more days, the optimization becomes more noticeable as only changed days redraw instead of the entire week.

## Cleanup
```bash
# Remove test database when done
pnpm dev db-admin --db test_opt --delete
```

## Advanced Testing

### Test Edge Cases:
1. **Changes outside date range** - Should be ignored
2. **Deleted documents** - Should trigger day updates
3. **Documents without occurTime** - Should be ignored
4. **Invalid time formats** - Should be handled gracefully

### Monitor Performance:
- Watch CPU usage during redraws
- Notice faster updates when only 1-2 days change vs full week
- Observe less visual flickering in the output file

## Troubleshooting

If you don't see the expected messages:
1. Make sure you're using `--watch` flag
2. Check that entries are being added successfully
3. Ensure the occur-time is within the displayed date range
4. Verify the database connection is working
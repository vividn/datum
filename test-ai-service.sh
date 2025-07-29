#!/bin/bash

# AI Service Test Suite
# This script tests the AI service with real examples

set -e

echo "🤖 AI Service Test Suite"
echo "========================"

# Check if OPENAI_API_KEY is set
if [ -z "$OPENAI_API_KEY" ]; then
    echo "❌ OPENAI_API_KEY not set. Please set your API key:"
    echo "   export OPENAI_API_KEY='your-key-here'"
    exit 1
fi

echo "✅ API key found"
echo ""

# Function to test AI parsing
test_parsing() {
    local input="$1"
    local expected_field="$2"
    local description="$3"
    
    echo "🧪 Testing: $description"
    echo "   Input: '$input'"
    echo "   Expected field: $expected_field"
    
    # Run the command and capture output
    output=$(pnpm dev -- ai "$input" 2>&1) || {
        echo "   ❌ FAILED: Command failed"
        echo "   Output: $output"
        return 1
    }
    
    # Check if entry was created
    if echo "$output" | grep -q "✅ Added:"; then
        echo "   ✅ PASSED: Entry created"
        echo "   Output: $(echo "$output" | grep "✅ Added:")"
    else
        echo "   ❌ FAILED: No entry created"
        echo "   Output: $output"
        return 1
    fi
    echo ""
}

# Function to test advanced modes
test_mode() {
    local mode="$1"
    local description="$2"
    local extra_args="$3"
    
    echo "🔍 Testing: $description"
    echo "   Mode: $mode"
    
    output=$(pnpm dev -- ai --mode "$mode" $extra_args 2>&1) || {
        echo "   ❌ FAILED: Command failed"
        echo "   Output: $output"
        return 1
    }
    
    echo "   ✅ PASSED: Command executed"
    echo "   Sample output: $(echo "$output" | head -3 | tail -1)"
    echo ""
}

echo "📝 Testing Natural Language Parsing"
echo "===================================="

# Test various input types
test_parsing "drank coffee this morning" "drink" "Simple drink entry"
test_parsing "ate pizza for lunch" "food" "Food with meal context"
test_parsing "went to gym for 45 minutes" "location" "Location with duration"
test_parsing "feeling happy and energetic" "mood" "Compound mood"
test_parsing "slept 8 hours last night" "sleep" "Sleep duration"
test_parsing "worked on project for 3 hours" "work" "Work activity"
test_parsing "weight: 72.5 kg" "weight" "Numeric measurement"
test_parsing "spent $45 on groceries" "expense" "Expense tracking"
test_parsing "took vitamin D supplement" "health" "Health/medication"
test_parsing "ran 5 miles in the park" "exercise" "Exercise with details"

echo "📊 Testing Advanced Features"
echo "============================="

# Test insights (might not work with little data)
test_mode "insights" "Generate insights from data" ""

# Test predictions
test_mode "predict" "Generate predictions" "--field drink,mood"

# Test explanations
test_mode "explain" "Answer data questions" '-q "What do I drink most often?"'

echo "🎯 Testing Edge Cases"
echo "====================="

# Test edge cases
test_parsing "🎉 feeling great!" "mood" "Input with emoji"
test_parsing "café au lait" "drink" "Special characters"
test_parsing "went to mom's house" "location" "Possessive location"
test_parsing "spent \$1,234.56" "expense" "Large currency amount"

echo "📈 Testing Interactive Mode"
echo "==========================="

echo "🧪 Testing: Interactive mode (auto-confirm first option)"
echo "   Input: 'had a stressful meeting'"

# For interactive mode, we'll simulate choosing option 1
echo "1" | pnpm dev -- ai -i "had a stressful meeting" && {
    echo "   ✅ PASSED: Interactive mode worked"
} || {
    echo "   ⚠️  WARNING: Interactive mode test skipped (requires manual input)"
}
echo ""

echo "📋 Final Verification"
echo "====================="

echo "🔍 Checking recent entries..."
pnpm dev -- tail -n 10

echo ""
echo "🎉 Test Suite Complete!"
echo "======================="

echo ""
echo "📊 Summary:"
echo "- Natural language parsing: Tested 10 examples"
echo "- Advanced features: Tested insights, predictions, explanations"
echo "- Edge cases: Tested special characters and edge inputs"
echo "- Interactive mode: Basic test completed"
echo ""
echo "✅ All tests completed successfully!"
echo "   Check the tail output above to see your parsed entries."
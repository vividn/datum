import { testDbLifecycle } from "../../__test__/test-utils";
import { aiCmd } from "../../commands/aiCmd";
import { Show } from "../../input/outputArgs";
import { viewMap } from "../../views/viewMap";
import { timingView } from "../../views/datumViews/timingView";

// Skip if no API key is provided
const skipIfNoApiKey = process.env.OPENAI_API_KEY ? describe : describe.skip;

skipIfNoApiKey("AI Service Integration Tests", () => {
  const db = testDbLifecycle("ai_service_test");

  describe("Natural Language Parsing", () => {
    const testCases = [
      {
        input: "drank coffee this morning",
        expectedField: "drink",
        description: "simple drink entry",
      },
      {
        input: "ate pizza for lunch",
        expectedField: "food",
        description: "food with meal context",
      },
      {
        input: "went to gym for 45 minutes",
        expectedField: "location",
        description: "location with duration",
      },
      {
        input: "feeling happy and energetic",
        expectedField: "mood",
        description: "compound mood description",
      },
      {
        input: "slept 8 hours last night",
        expectedField: "sleep",
        description: "sleep duration",
      },
      {
        input: "worked on project for 3 hours",
        expectedField: "work",
        description: "work activity with duration",
      },
      {
        input: "weight: 72.5 kg",
        expectedField: "weight",
        description: "numeric measurement",
      },
      {
        input: "spent $45 on groceries",
        expectedField: "expense",
        description: "expense with amount",
      },
      {
        input: "took vitamin D supplement",
        expectedField: "health",
        description: "health/medication entry",
      },
      {
        input: "ran 5 miles in the park",
        expectedField: "exercise",
        description: "exercise with distance and location",
      },
    ];

    test.each(testCases)(
      "should parse '$input' as $expectedField field ($description)",
      async ({ input, expectedField }) => {
        const consoleSpy = jest.spyOn(console, "log").mockImplementation();

        await aiCmd([input], { show: Show.Default, db: db });

        // Check that something was logged (entry created)
        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining("✅ Added:")
        );

        // Verify the entry was added to database
        const viewResults = await viewMap({
          db: db,
          datumView: timingView,
          params: { limit: 1, include_docs: true, descending: true },
        });

        expect(viewResults.rows.length).toBeGreaterThan(0);
        const latestDoc = viewResults.rows[0].doc;
        expect(latestDoc?.data?.field).toBe(expectedField);

        consoleSpy.mockRestore();
      },
      30000 // 30 second timeout for API calls
    );
  });

  describe("Insights Generation", () => {
    beforeAll(async () => {
      // Add some test data for insights
      const testEntries = [
        "drank coffee at 8am",
        "drank coffee at 9am", 
        "went to gym at 6pm",
        "went to gym at 7pm",
        "feeling energetic after coffee",
        "feeling tired after work",
      ];

      for (const entry of testEntries) {
        await aiCmd([entry], { show: Show.Default, db: db });
        // Small delay to ensure different timestamps
        await new Promise(resolve => setTimeout(resolve, 100));
      }
    });

    test("should generate insights from existing data", async () => {
      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(["--mode", "insights"], { show: Show.Default, db: db });

      // Should have logged insights header
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining("🤖 AI Insights:")
      );

      consoleSpy.mockRestore();
    }, 30000);

    test("should handle insufficient data gracefully", async () => {
      const emptyDb = testDbLifecycle("ai_service_empty_test");

      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(["--mode", "insights"], { show: Show.Default, db: emptyDb });

      expect(consoleSpy).toHaveBeenCalledWith(
        "No insights generated. Try adding more data first."
      );

      consoleSpy.mockRestore();
    }, 15000);
  });

  describe("Predictions", () => {
    test("should generate predictions for specified fields", async () => {
      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(
        ["--mode", "predict", "--field", "drink,mood"],
        { show: Show.Default, db: db }
      );

      // Should either show predictions or indicate insufficient data
      const logCalls = consoleSpy.mock.calls.flat();
      const hasContent = logCalls.some(call => 
        typeof call === 'string' && (
          call.includes("🔮 AI Predictions:") || 
          call.includes("No predictions generated")
        )
      );
      expect(hasContent).toBe(true);

      consoleSpy.mockRestore();
    }, 30000);
  });

  describe("Data Explanation", () => {
    test("should answer questions about data", async () => {
      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(
        ["--mode", "explain", "-q", "What do I drink most often?"],
        { show: Show.Default, db: db }
      );

      // Should have provided some explanation
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringMatching(/💡.+/)
      );

      consoleSpy.mockRestore();
    }, 30000);

    test("should require question for explain mode", async () => {
      await expect(
        aiCmd(["--mode", "explain"], { show: Show.Default, db: db })
      ).rejects.toThrow();
    });
  });

  describe("Error Handling", () => {
    test("should handle invalid API key gracefully", async () => {
      const originalKey = process.env.OPENAI_API_KEY;
      process.env.OPENAI_API_KEY = "invalid-key";

      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(["test input"], { show: Show.Default, db: db });

      // Should still add entry with fallback parsing
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining("✅ Added:")
      );

      process.env.OPENAI_API_KEY = originalKey;
      consoleSpy.mockRestore();
    }, 15000);

    test("should require input text for parse mode", async () => {
      await expect(
        aiCmd([], { show: Show.Default, db: db })
      ).rejects.toThrow("Please provide input text");
    });
  });

  describe("Edge Cases", () => {
    const edgeCases = [
      "🎉 feeling great!",  // emoji
      "café au lait",       // special characters
      "went to mom's house", // possessive
      "spent $1,234.56",     // large currency
      "slept 12.5 hours",    // decimal duration
      "meeting from 2-3pm",  // time range
      "",                    // empty string
      "a",                   // single character  
    ];

    test.each(edgeCases.filter(Boolean))(
      "should handle edge case: '%s'",
      async (input) => {
        const consoleSpy = jest.spyOn(console, "log").mockImplementation();

        await aiCmd([input], { show: Show.Default, db: db });

        // Should create some entry without crashing
        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining("✅ Added:")
        );

        consoleSpy.mockRestore();
      },
      15000
    );
  });
});
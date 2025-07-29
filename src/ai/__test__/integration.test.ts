import { AIService } from "../aiService";

// Skip if no API key is provided
const skipIfNoApiKey = process.env.OPENAI_API_KEY ? describe : describe.skip;

skipIfNoApiKey("AI Service Simple Integration Tests", () => {
  let aiService: AIService;

  beforeEach(() => {
    aiService = new AIService({
      apiKey: process.env.OPENAI_API_KEY!,
      model: "gpt-4o-mini",
    });
  });

  describe("Natural Language Parsing", () => {
    const testCases = [
      { input: "drank coffee this morning", expectedField: "drink" },
      { input: "ate pizza for lunch", expectedField: "food" },
      { input: "went to gym", expectedField: "exercise" },
      { input: "feeling happy", expectedField: "mood" },
      { input: "slept 8 hours", expectedField: "sleep" },
    ];

    test.each(testCases)(
      "should parse '$input' as $expectedField field",
      async ({ input, expectedField }) => {
        const result = await aiService.parseNaturalLanguage(input);
        
        expect(result.field).toBe(expectedField);
        expect(result.raw).toBe(input);
        expect(result.confidence).toBeGreaterThan(0.5);
        expect(result.value).toBeDefined();
      },
      15000 // 15 second timeout for API calls
    );
  });

  describe("Error Handling", () => {
    test("should handle API errors gracefully", async () => {
      const invalidService = new AIService({
        apiKey: "invalid-key",
        model: "gpt-4o-mini",
      });

      const result = await invalidService.parseNaturalLanguage("test input");
      
      // Should still return a result with fallback parsing
      expect(result.field).toBe("note");
      expect(result.value).toBe("test input");
      expect(result.confidence).toBe(0.3);
    }, 10000);
  });

  describe("Data Explanation", () => {
    test("should provide explanations for questions", async () => {
      const mockDocs = [
        {
          _id: "test1",
          _rev: "1",
          data: { field: "drink", value: "coffee" },
          meta: { createTime: { utc: "2025-01-01T08:00:00Z" } },
        },
        {
          _id: "test2", 
          _rev: "1",
          data: { field: "drink", value: "tea" },
          meta: { createTime: { utc: "2025-01-01T14:00:00Z" } },
        },
      ];

      const explanation = await aiService.explainData(
        mockDocs as any,
        "What do I drink?"
      );

      expect(typeof explanation).toBe("string");
      expect(explanation.length).toBeGreaterThan(10);
    }, 15000);
  });
});
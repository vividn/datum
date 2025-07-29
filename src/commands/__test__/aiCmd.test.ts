import { aiCmd } from "../aiCmd";
import { TestDbLifecycle } from "../../__test__/test-utils";
import { AIService } from "../../ai/aiService";
import prompts from "prompts";

jest.mock("../../ai/aiService");
jest.mock("prompts");

describe("aiCmd", () => {
  const testDb = new TestDbLifecycle();

  beforeEach(() => {
    jest.clearAllMocks();
    process.env.OPENAI_API_KEY = "test-api-key";
  });

  afterEach(() => {
    delete process.env.OPENAI_API_KEY;
  });

  describe("parse mode (default)", () => {
    it("should parse natural language input", async () => {
      const mockParsedEntry = {
        field: "location",
        value: "gym",
        confidence: 0.9,
        raw: "went to gym",
      };

      const mockAIService = {
        parseNaturalLanguage: jest.fn().mockResolvedValue(mockParsedEntry),
      };
      
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation(() => mockAIService as any);

      const result = await aiCmd(
        ["went", "to", "gym"],
        { show: "default", db: testDb.db },
      );

      expect(mockAIService.parseNaturalLanguage).toHaveBeenCalled();
      expect(result).toBeDefined();
    });

    it("should handle interactive mode", async () => {
      const mockParsedEntries = [
        { field: "location", value: "gym", confidence: 0.9, raw: "went to gym" },
        { field: "exercise", value: "gym", confidence: 0.7, raw: "went to gym" },
      ];

      const mockAIService = {
        parseNaturalLanguage: jest.fn()
          .mockResolvedValueOnce(mockParsedEntries[0])
          .mockResolvedValueOnce({ value: { alternatives: [mockParsedEntries[1]] } }),
      };
      
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation(() => mockAIService as any);
      (prompts as jest.MockedFunction<typeof prompts>).mockResolvedValue({ choice: 0 });

      await aiCmd(
        ["-i", "went", "to", "gym"],
        { show: "default", db: testDb.db },
      );

      expect(prompts).toHaveBeenCalled();
    });

    it("should throw error when no API key provided", async () => {
      delete process.env.OPENAI_API_KEY;
      
      await expect(
        aiCmd(["test"], { show: "default", db: testDb.db })
      ).rejects.toThrow("OpenAI API key required");
    });
  });

  describe("insights mode", () => {
    it("should generate insights from data", async () => {
      const mockInsights = [
        {
          type: "pattern",
          field: "exercise",
          description: "You exercise most frequently on weekdays",
          confidence: 0.8,
        },
      ];

      const mockAIService = {
        generateInsights: jest.fn().mockResolvedValue(mockInsights),
      };
      
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation(() => mockAIService as any);

      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(
        ["--mode", "insights"],
        { show: "default", db: testDb.db },
      );

      expect(mockAIService.generateInsights).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining("AI Insights"));
      
      consoleSpy.mockRestore();
    });

    it("should handle empty insights", async () => {
      const mockAIService = {
        generateInsights: jest.fn().mockResolvedValue([]),
      };
      
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation(() => mockAIService as any);

      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(
        ["--mode", "insights"],
        { show: "default", db: testDb.db },
      );

      expect(consoleSpy).toHaveBeenCalledWith("No insights generated. Try adding more data first.");
      
      consoleSpy.mockRestore();
    });
  });

  describe("predict mode", () => {
    it("should generate predictions", async () => {
      const mockPredictions = [
        {
          field: "weight",
          predictedValue: 72.5,
          confidence: 0.75,
          date: { human: "2024-01-20", utc: "2024-01-20T00:00:00Z" },
          basedOn: { dataPoints: 30, method: "linear" },
        },
      ];

      const mockAIService = {
        generatePredictions: jest.fn().mockResolvedValue(mockPredictions),
      };
      
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation(() => mockAIService as any);

      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(
        ["--mode", "predict", "--field", "weight"],
        { show: "default", db: testDb.db },
      );

      expect(mockAIService.generatePredictions).toHaveBeenCalledWith(
        expect.any(Array),
        ["weight"],
      );
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining("AI Predictions"));
      
      consoleSpy.mockRestore();
    });
  });

  describe("explain mode", () => {
    it("should answer questions about data", async () => {
      const mockExplanation = "You usually exercise in the morning between 6-8am.";

      const mockAIService = {
        explainData: jest.fn().mockResolvedValue(mockExplanation),
      };
      
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation(() => mockAIService as any);

      const consoleSpy = jest.spyOn(console, "log").mockImplementation();

      await aiCmd(
        ["--mode", "explain", "-q", "When do I exercise?"],
        { show: "default", db: testDb.db },
      );

      expect(mockAIService.explainData).toHaveBeenCalledWith(
        expect.any(Array),
        "When do I exercise?",
      );
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining(mockExplanation));
      
      consoleSpy.mockRestore();
    });

    it("should require question for explain mode", async () => {
      await expect(
        aiCmd(
          ["--mode", "explain"],
          { show: "default", db: testDb.db },
        )
      ).rejects.toThrow("Please provide input text or use --mode");
    });
  });

  describe("input handling", () => {
    it("should require input for parse mode", async () => {
      await expect(
        aiCmd([], { show: "default", db: testDb.db })
      ).rejects.toThrow("Please provide input text or use --mode");
    });

    it("should handle multi-word input", async () => {
      const mockParsedEntry = {
        field: "note",
        value: "this is a long multi word input",
        confidence: 0.6,
        raw: "this is a long multi word input",
      };

      const mockAIService = {
        parseNaturalLanguage: jest.fn().mockResolvedValue(mockParsedEntry),
      };
      
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation(() => mockAIService as any);

      await aiCmd(
        ["this", "is", "a", "long", "multi", "word", "input"],
        { show: "default", db: testDb.db },
      );

      expect(mockAIService.parseNaturalLanguage).toHaveBeenCalledWith(
        "this is a long multi word input",
        expect.any(Array),
      );
    });
  });

  describe("model configuration", () => {
    it("should use custom model when specified", async () => {
      const mockAIService = jest.fn();
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation((config) => {
        mockAIService(config);
        return {
          parseNaturalLanguage: jest.fn().mockResolvedValue({
            field: "note",
            value: "test",
            confidence: 0.5,
            raw: "test",
          }),
        } as any;
      });

      await aiCmd(
        ["--model", "gpt-4", "test"],
        { show: "default", db: testDb.db },
      );

      expect(mockAIService).toHaveBeenCalledWith(
        expect.objectContaining({ model: "gpt-4" }),
      );
    });

    it("should use API key from argument when provided", async () => {
      delete process.env.OPENAI_API_KEY;
      
      const mockAIService = jest.fn();
      (AIService as jest.MockedClass<typeof AIService>).mockImplementation((config) => {
        mockAIService(config);
        return {
          parseNaturalLanguage: jest.fn().mockResolvedValue({
            field: "note",
            value: "test",
            confidence: 0.5,
            raw: "test",
          }),
        } as any;
      });

      await aiCmd(
        ["--api-key", "custom-key", "test"],
        { show: "default", db: testDb.db },
      );

      expect(mockAIService).toHaveBeenCalledWith(
        expect.objectContaining({ apiKey: "custom-key" }),
      );
    });
  });
});
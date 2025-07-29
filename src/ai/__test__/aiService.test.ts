import { AIService } from "../aiService";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { toDatumTime } from "../../time/datumTime";
import { DateTime } from "luxon";

jest.mock("openai");

describe("AIService", () => {
  let aiService: AIService;
  const mockApiKey = "test-api-key";

  beforeEach(() => {
    aiService = new AIService({
      apiKey: mockApiKey,
      provider: "openai",
      model: "gpt-4o-mini",
    });
  });

  describe("parseNaturalLanguage", () => {
    it("should parse natural language input without API when no AI provider configured", async () => {
      const serviceWithoutAPI = new AIService({ provider: "claude" });
      
      await expect(
        serviceWithoutAPI.parseNaturalLanguage("went to gym"),
      ).rejects.toThrow("NLP parsing not enabled or AI provider not configured");
    });

    it("should return fallback when API fails", async () => {
      const mockOpenAI = {
        chat: {
          completions: {
            create: jest.fn().mockRejectedValue(new Error("API Error")),
          },
        },
      };
      (aiService as any).openai = mockOpenAI;

      const result = await aiService.parseNaturalLanguage("test input");
      
      expect(result).toMatchObject({
        field: "note",
        value: "test input",
        confidence: 0.3,
        raw: "test input",
      });
    });
  });

  describe("generateInsights", () => {
    it("should return empty array when insights disabled", async () => {
      const serviceNoInsights = new AIService({
        apiKey: mockApiKey,
        enableInsights: false,
      });

      const docs: DatumDocument[] = [];
      const db = {} as PouchDB.Database;
      
      const insights = await serviceNoInsights.generateInsights(docs, db);
      expect(insights).toEqual([]);
    });

    it("should return empty array with insufficient data", async () => {
      const docs: DatumDocument[] = [
        {
          _id: "test1",
          _rev: "1",
          data: "test",
          meta: {
            createTime: toDatumTime(DateTime.now().toISO()),
          },
        },
      ];
      const db = {} as PouchDB.Database;
      
      const insights = await aiService.generateInsights(docs, db);
      expect(insights).toEqual([]);
    });
  });

  describe("generatePredictions", () => {
    it("should return empty array when predictions disabled", async () => {
      const serviceNoPredictions = new AIService({
        apiKey: mockApiKey,
        enablePredictions: false,
      });

      const docs: DatumDocument[] = [];
      const predictions = await serviceNoPredictions.generatePredictions(docs, ["weight"]);
      expect(predictions).toEqual([]);
    });

    it("should skip fields with insufficient data", async () => {
      const docs: DatumDocument[] = [];
      const predictions = await aiService.generatePredictions(docs, ["weight"]);
      expect(predictions).toEqual([]);
    });
  });

  describe("explainData", () => {
    it("should throw when AI provider not configured", async () => {
      const serviceNoAPI = new AIService({ provider: "claude" });
      
      await expect(
        serviceNoAPI.explainData([], "test question"),
      ).rejects.toThrow("AI provider not configured");
    });

    it("should handle API errors gracefully", async () => {
      const mockOpenAI = {
        chat: {
          completions: {
            create: jest.fn().mockRejectedValue(new Error("API Error")),
          },
        },
      };
      (aiService as any).openai = mockOpenAI;

      const result = await aiService.explainData([], "test question");
      expect(result).toBe("Failed to analyze data due to an error.");
    });
  });
});
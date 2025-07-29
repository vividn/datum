import OpenAI from "openai";
import { AIServiceConfig, ParsedEntry, AIInsight, Prediction } from "./types";
import { DatumDocument } from "../documentControl/DatumDocument";
import { datumTime, DatumTime } from "../time/datumTime";
import { DateTime } from "luxon";

export class AIService {
  private config: AIServiceConfig;
  private openai?: OpenAI;

  constructor(config: Partial<AIServiceConfig> = {}) {
    this.config = {
      enableNLP: true,
      enableInsights: true,
      enablePredictions: true,
      insightLookbackDays: 30,
      predictionHorizonDays: 7,
      minConfidenceThreshold: 0.6,
      provider: "openai",
      model: "gpt-4o-mini",
      ...config,
    };

    if (this.config.provider === "openai" && this.config.apiKey) {
      this.openai = new OpenAI({ apiKey: this.config.apiKey });
    }
  }

  async parseNaturalLanguage(input: string, context?: DatumDocument[]): Promise<ParsedEntry> {
    if (!this.config.enableNLP || !this.openai) {
      throw new Error("NLP parsing not enabled or OpenAI not configured");
    }

    const systemPrompt = `You are a data entry parser for a life tracking application called Datum.
Parse natural language input into structured data entries.
Available fields include: location, food, drink, sleep, exercise, expense, mood, weight, start, end, and custom fields.
Return JSON with: field (string), value (any), time (ISO string or null), duration (minutes or null), confidence (0-1).
Context: Current time is ${new Date().toISOString()}`;

    const userPrompt = context?.length
      ? `Recent entries for context:\n${context
          .slice(-5)
          .map((d) => `${d.meta?.field}: ${JSON.stringify(d.data)}`)
          .join("\n")}\n\nParse: "${input}"`
      : `Parse: "${input}"`;

    try {
      const response = await this.openai.chat.completions.create({
        model: this.config.model || "gpt-4o-mini",
        messages: [
          { role: "system", content: systemPrompt },
          { role: "user", content: userPrompt },
        ],
        response_format: { type: "json_object" },
        temperature: 0.3,
      });

      const parsed = JSON.parse(response.choices[0].message.content || "{}");
      
      return {
        field: parsed.field || "note",
        value: parsed.value,
        time: parsed.time ? datumTime(parsed.time) : datumTime(),
        duration: parsed.duration,
        confidence: parsed.confidence || 0.8,
        raw: input,
      };
    } catch (error) {
      console.error("Failed to parse with AI:", error);
      return {
        field: "note",
        value: input,
        time: datumTime(),
        confidence: 0.3,
        raw: input,
      };
    }
  }

  async generateInsights(
    documents: DatumDocument[],
    db: PouchDB.Database,
  ): Promise<AIInsight[]> {
    if (!this.config.enableInsights || !this.openai) {
      return [];
    }

    const cutoffTime = DateTime.now().minus({ days: this.config.insightLookbackDays });
    const recentDocs = documents.filter((doc) => {
      const docTime = doc.meta?.occurTime || doc.meta?.createTime;
      return docTime && DateTime.fromISO(docTime.utc) > cutoffTime;
    });

    if (recentDocs.length < 10) {
      return [];
    }

    const fieldGroups = this.groupByField(recentDocs);
    const insights: AIInsight[] = [];

    const systemPrompt = `You are an AI analyst for a life tracking application.
Analyze user data to find patterns, anomalies, correlations, and provide suggestions.
Return JSON array of insights with: type, field/fields, description, confidence (0-1), data (optional).
Focus on actionable, meaningful insights. Limit to 5 most important insights.`;

    const dataSnapshot = Object.entries(fieldGroups)
      .map(([field, docs]) => ({
        field,
        count: docs.length,
        samples: docs.slice(-5).map((d) => ({
          time: d.meta?.occurTime?.utc,
          value: d.data,
        })),
      }))
      .filter((g) => g.count > 3);

    try {
      const response = await this.openai.chat.completions.create({
        model: this.config.model || "gpt-4o-mini",
        messages: [
          { role: "system", content: systemPrompt },
          {
            role: "user",
            content: `Analyze this data from the last ${this.config.insightLookbackDays} days:\n${JSON.stringify(
              dataSnapshot,
              null,
              2,
            )}`,
          },
        ],
        response_format: { type: "json_object" },
        temperature: 0.5,
      });

      const aiInsights = JSON.parse(response.choices[0].message.content || "{}");
      const parsedInsights = (aiInsights.insights || []).map((insight: any) => ({
        type: insight.type || "pattern",
        field: insight.field,
        fields: insight.fields,
        description: insight.description,
        confidence: insight.confidence || 0.7,
        data: insight.data,
      }));

      return parsedInsights.filter((i: AIInsight) => i.confidence >= this.config.minConfidenceThreshold);
    } catch (error) {
      console.error("Failed to generate insights:", error);
      return [];
    }
  }

  async generatePredictions(
    documents: DatumDocument[],
    fields: string[],
  ): Promise<Prediction[]> {
    if (!this.config.enablePredictions || !this.openai) {
      return [];
    }

    const predictions: Prediction[] = [];
    const fieldGroups = this.groupByField(documents);

    for (const field of fields) {
      const fieldDocs = fieldGroups[field] || [];
      if (fieldDocs.length < 7) continue;

      const systemPrompt = `You are a predictive analytics AI for life tracking data.
Analyze historical data to predict future values.
Return JSON with predictions array containing: date (ISO string), value, confidence (0-1), method.
Base predictions on patterns, trends, and seasonality in the data.
Predict for the next ${this.config.predictionHorizonDays} days.`;

      const historicalData = fieldDocs
        .slice(-30)
        .map((d) => ({
          date: d.meta?.occurTime?.utc || d.meta?.createTime?.utc,
          value: d.data,
        }))
        .filter((d) => d.date);

      try {
        const response = await this.openai.chat.completions.create({
          model: this.config.model || "gpt-4o-mini",
          messages: [
            { role: "system", content: systemPrompt },
            {
              role: "user",
              content: `Predict future values for field "${field}" based on:\n${JSON.stringify(
                historicalData,
                null,
                2,
              )}`,
            },
          ],
          response_format: { type: "json_object" },
          temperature: 0.4,
        });

        const result = JSON.parse(response.choices[0].message.content || "{}");
        const fieldPredictions = (result.predictions || []).map((p: any) => ({
          field,
          predictedValue: p.value,
          confidence: p.confidence || 0.6,
          date: datumTime(p.date),
          basedOn: {
            dataPoints: historicalData.length,
            method: p.method || "ml",
          },
        }));

        predictions.push(
          ...fieldPredictions.filter((p: Prediction) => p.confidence >= this.config.minConfidenceThreshold),
        );
      } catch (error) {
        console.error(`Failed to generate predictions for ${field}:`, error);
      }
    }

    return predictions;
  }

  private groupByField(documents: DatumDocument[]): Record<string, DatumDocument[]> {
    const groups: Record<string, DatumDocument[]> = {};
    
    for (const doc of documents) {
      const field = doc.meta?.field || "unknown";
      if (!groups[field]) {
        groups[field] = [];
      }
      groups[field].push(doc);
    }
    
    return groups;
  }

  async explainData(
    documents: DatumDocument[],
    question: string,
  ): Promise<string> {
    if (!this.openai) {
      throw new Error("OpenAI not configured");
    }

    const systemPrompt = `You are a helpful assistant analyzing life tracking data.
Answer questions about the user's data patterns, trends, and insights.
Be concise and specific. Reference actual data points when relevant.`;

    const dataSnapshot = documents.slice(-50).map((d) => ({
      field: d.meta?.field,
      time: d.meta?.occurTime?.utc,
      value: d.data,
    }));

    try {
      const response = await this.openai.chat.completions.create({
        model: this.config.model || "gpt-4o-mini",
        messages: [
          { role: "system", content: systemPrompt },
          {
            role: "user",
            content: `Data: ${JSON.stringify(dataSnapshot, null, 2)}\n\nQuestion: ${question}`,
          },
        ],
        temperature: 0.5,
      });

      return response.choices[0].message.content || "Unable to analyze data.";
    } catch (error) {
      console.error("Failed to explain data:", error);
      return "Failed to analyze data due to an error.";
    }
  }
}
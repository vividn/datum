import { DatumTime } from "../time/datumTime";

export interface ParsedEntry {
  field: string;
  value?: unknown;
  time?: DatumTime;
  duration?: number;
  confidence: number;
  raw: string;
}

export interface AIInsight {
  type: "pattern" | "anomaly" | "correlation" | "suggestion";
  field?: string;
  fields?: string[];
  description: string;
  confidence: number;
  timeRange?: {
    start: DatumTime;
    end: DatumTime;
  };
  data?: unknown;
}

export interface Prediction {
  field: string;
  predictedValue: unknown;
  confidence: number;
  date: DatumTime;
  basedOn: {
    dataPoints: number;
    method: "linear" | "seasonal" | "ml";
  };
}

export interface AIServiceConfig {
  enableNLP: boolean;
  enableInsights: boolean;
  enablePredictions: boolean;
  insightLookbackDays: number;
  predictionHorizonDays: number;
  minConfidenceThreshold: number;
  provider: "openai" | "claude" | "local";
  apiKey?: string;
  localModelPath?: string;
  model?: string;
}

export interface NLPPattern {
  pattern: RegExp;
  extractor: (match: RegExpMatchArray) => Partial<ParsedEntry>;
  examples: string[];
}
import { ParsedEntry } from "./types";
import { AIService } from "./aiService";
import { DatumDocument } from "../documentControl/DatumDocument";

export async function parseNaturalLanguage(
  input: string,
  aiService: AIService,
  context?: DatumDocument[],
): Promise<ParsedEntry> {
  return aiService.parseNaturalLanguage(input, context);
}

export async function suggestInterpretations(
  input: string,
  aiService: AIService,
  context?: DatumDocument[],
): Promise<ParsedEntry[]> {
  const primary = await parseNaturalLanguage(input, aiService, context);
  
  const alternatives: ParsedEntry[] = [primary];
  
  if (primary.confidence < 0.9) {
    const altPrompt = `Suggest 2-3 alternative interpretations for: "${input}"`;
    try {
      const altEntry = await aiService.parseNaturalLanguage(altPrompt, context);
      if (altEntry.value && typeof altEntry.value === "object" && "alternatives" in altEntry.value) {
        const alts = (altEntry.value as any).alternatives as ParsedEntry[];
        alternatives.push(...alts);
      }
    } catch (error) {
      console.error("Failed to get alternative interpretations:", error);
    }
  }
  
  return alternatives.sort((a, b) => b.confidence - a.confidence);
}
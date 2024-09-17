import { isoDatetime } from "../time/timeUtils";

export function extractTimeFromId(id: string): isoDatetime | null {
  return id.match(/\d{4}-\d{2}-\d{2}T[0-9:.]+Z/)?.[0] || null;
}

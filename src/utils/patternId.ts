import { DatumState } from "../state/normalizeState";

export function patternId(states: DatumState[]): string {
  const stateStr = states.map((s) => s?.toString() || "null").join("_");
  return `pattern_${stateStr.replace(/[^a-zA-Z0-9]/g, "_")}`;
}

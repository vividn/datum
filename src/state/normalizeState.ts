import { DatumState } from "../views/datumViews/activeStateView";

export function normalizeState(state?: unknown): DatumState | undefined {
  if (state === undefined) {
    return undefined;
  }
  if (state === null) {
    return null;
  }
  if (
    state === undefined ||
    state === null ||
    typeof state === "string" ||
    Array.isArray(state) ||
    typeof state === "boolean"
  ) {
    return state;
  }
}


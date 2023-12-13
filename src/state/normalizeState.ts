import { DatumState } from "../views/datumViews/activeStateView";

export function normalizeState(state?: ): DatumState | undefined {
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

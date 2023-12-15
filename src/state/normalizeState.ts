import { DatumState } from "../views/datumViews/activeStateView";
import flattenDeep from "lodash/flattenDeep";
import { BadStateError } from "../errors";
import { JsonType } from "../utils/utilityTypes";
import isEqual from "lodash.isequal";
export function normalizeState(state: JsonType): DatumState {
  if (state === null) {
    return { id: null };
  }
  if (Array.isArray(state)) {
    if (state.length === 0) {
      return false;
    }
    if (state.length === 1) {
      return normalizeState(state[0]);
    }
    return flattenDeep(
      state.map((innerState) => {
        const normalized = normalizeState(innerState);
        if (isEqual(normalized, { id: null })) {
          throw new BadStateError(
            "null is a special state to indicate that the field is not being tracked and cannot exist together with other states",
          );
        }
        return normalized;
      }),
    );
  }
  if (
    typeof state === "number" ||
    typeof state === "string" ||
    typeof state === "boolean"
  ) {
    return { id: state };
  }
  if (state.id === undefined) {
    return { id: true, ...state };
  }
  const id = state.id as JsonType;
  const otherKeys = { ...state };
  delete otherKeys.id;

  if (
    typeof id === "number" ||
    typeof id === "string" ||
    typeof id === "boolean"
  ) {
    return { id, ...otherKeys };
  }
  const normalizedId = normalizeState(id);
  if (isEqual(normalizedId, { id: null })) {
    if (Object.keys(otherKeys).length > 0) {
      console.warn(
        "null is a special state to indicate that the field is not being tracked, but additional state data was provided. It has been removed",
      );
    }
    return { id: null };
  }

  if (Array.isArray(normalizedId)) {
    return flattenDeep(
      normalizedId.map((innerState) => ({
        ...innerState,
        ...otherKeys,
      })),
    );
  }
  return { ...normalizedId, ...otherKeys };
}

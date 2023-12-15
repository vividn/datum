import flattenDeep from "lodash/flattenDeep";
import { BadStateError } from "../errors";
import { JsonType } from "../utils/utilityTypes";
import { GenericObject } from "../GenericObject";

export type StateObject = GenericObject & {
  id: string | boolean | number;
};
export type SingleState = string | boolean | number | StateObject;
export type DatumState = null | SingleState | SingleState[];

export function normalizeState(state: JsonType): DatumState {
  if (state === null) {
    return null;
  }
  if (
    typeof state === "number" ||
    typeof state === "string" ||
    typeof state === "boolean"
  ) {
    return state;
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
        if (normalized === null) {
          throw new BadStateError(
            "null is a special state to indicate that the field is not being tracked and cannot exist together with other states",
          );
        }
        return normalized;
      }),
    );
  }
  if (state.id === undefined) {
    state.id = true;
  }
  const id = state.id as JsonType;
  const otherKeys = { ...state };
  delete otherKeys.id;

  if (Object.keys(otherKeys).length === 0) {
    return normalizeState(id);
  }
  const normalizedId = normalizeState(id);
  if (normalizedId === null) {
    if (Object.keys(otherKeys).length > 0) {
      console.warn(
        "null is a special state to indicate that the field is not being tracked, but additional state data was provided. It has been removed",
      );
    }
    return null;
  }
  if (
    typeof normalizedId === "number" ||
    typeof normalizedId === "string" ||
    typeof normalizedId === "boolean"
  ) {
    return { id: normalizedId, ...otherKeys};
  }

  if (Array.isArray(normalizedId)) {
    return normalizedId.map((single) => {
      if (
        typeof single === "number" ||
        typeof single === "string" ||
        typeof single === "boolean"
      ) {
        return { id: single, ...otherKeys };
      }
      return { ...single, ...otherKeys };
    });
  }
  return { ...normalizedId, ...otherKeys };
}

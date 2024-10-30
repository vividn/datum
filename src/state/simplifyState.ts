import flattenDeep from "lodash.flattendeep";
import { DatumState } from "./normalizeState";

export type SimpleSingleState = string | boolean;
export type SimpleDatumState = null | SimpleSingleState | SimpleSingleState[];

export function simplifyState(state: DatumState): SimpleDatumState {
  if (state === null) {
    return null;
  }
  if (typeof state === "string" || typeof state === "boolean") {
    return state;
  }
  if (Array.isArray(state)) {
    if (state.length === 0) {
      return false;
    }
    if (state.length === 1) {
      return simplifyState(state[0]);
    }
    return flattenDeep(
      state.map((innerState) => {
        const simplified = simplifyState(innerState);
        if (simplified === null) {
          throw new Error(
            "null is a special state to indicate that the field is not being tracked and cannot exist together with other states",
          );
        }
        return simplified;
      }),
    );
  }
  if (state.id !== undefined) {
    return state.id;
  }
  return JSON.stringify(state);
}

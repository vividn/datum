import { jClone } from "../utils/jClone";
import isPlainObject from "lodash.isplainobject";
import { MergeError } from "../errors";
import { rekey } from "../utils/rekey";
import { GenericObject, GenericType, JsonObject } from "../utils/utilityTypes";

export type UpdateStrategyNames =
  | "useOld"
  | "useNew"
  | "preferOld"
  | "update"
  | "preferNew"
  | "intersection"
  | "removeConflicting"
  | "xor"
  | "merge"
  | "append"
  | "prepend"
  | "mergeSort"
  | "appendSort"
  | "rekey";

export type conflictingKeyStrategies =
  | false
  | "A"
  | "B"
  | "merge"
  | "append"
  | "prepend"
  | "mergeSort"
  | "appendSort";

type CombiningType = {
  justA: boolean;
  justB: boolean;
  same: boolean;
  conflict: conflictingKeyStrategies;
};

type CustomCombine = (aData: GenericObject, bData: GenericObject) => JsonObject;

export const updateStrategies: Record<
  UpdateStrategyNames,
  CombiningType | CustomCombine
> = {
  useOld: { justA: true, justB: false, same: true, conflict: "A" },
  useNew: { justA: false, justB: true, same: true, conflict: "B" },
  preferOld: { justA: true, justB: true, same: true, conflict: "A" },
  update: { justA: true, justB: true, same: true, conflict: "B" },
  preferNew: { justA: true, justB: true, same: true, conflict: "B" },
  intersection: { justA: false, justB: false, same: true, conflict: false },
  removeConflicting: { justA: true, justB: true, same: true, conflict: false },
  xor: { justA: true, justB: true, same: false, conflict: false },
  merge: { justA: true, justB: true, same: true, conflict: "merge" },
  append: { justA: true, justB: true, same: true, conflict: "append" },
  prepend: { justA: true, justB: true, same: true, conflict: "prepend" },
  mergeSort: { justA: true, justB: true, same: true, conflict: "mergeSort" },
  appendSort: { justA: true, justB: true, same: true, conflict: "appendSort" },
  rekey: rekey,
};

export function combineData(
  aData: GenericObject,
  bData: GenericObject,
  how: UpdateStrategyNames | CombiningType | CustomCombine,
): JsonObject {
  const strategy = typeof how === "string" ? updateStrategies[how] : how;
  const aClone = jClone(aData);
  const bClone = jClone(bData);

  if (typeof strategy === "function") {
    return strategy(aClone, bClone) as JsonObject;
  }

  const combined = {} as GenericObject;

  for (const key in aData) {
    const aVal = aClone[key];

    // Just in aData
    if (!(key in bData)) {
      if (strategy.justA) combined[key] = aVal;
      continue;
    }
    const bVal = bClone[key];
    delete bClone[key];

    // If objects in both, then recurse and run strategy on sub object
    if (isPlainObject(aVal) && isPlainObject(bVal)) {
      combined[key] = combineData(
        aVal as GenericObject,
        bVal as GenericObject,
        how,
      );
      continue;
    }

    // In both
    if (aVal === bVal) {
      if (strategy.same) combined[key] = aVal;
      continue;
    }

    // Data is different across a and b
    if (strategy.conflict === "A") {
      combined[key] = aVal;
    } else if (strategy.conflict === "B") {
      combined[key] = bVal;
    } else if (strategy.conflict === false) {
      // pass
    } else if (strategy.conflict === "merge") {
      combined[key] = mergeValues(aVal, bVal, true);
    } else if (strategy.conflict === "append") {
      combined[key] = mergeValues(aVal, bVal, false);
    } else if (strategy.conflict === "prepend") {
      combined[key] = mergeValues(bVal, aVal, false);
    } else if (strategy.conflict === "mergeSort") {
      combined[key] = mergeValues(aVal, bVal, true, true);
    } else if (strategy.conflict === "appendSort") {
      combined[key] = mergeValues(aVal, bVal, false, true);
    } else {
      throw new Error("unknown conflict resolution strategy");
    }
  }

  // just in bData
  if (strategy.justB) {
    for (const bKey in bClone) {
      combined[bKey] = bClone[bKey];
    }
  }

  return combined as JsonObject;
}

const isMergeableValue = (val: unknown) => {
  return (
    typeof val === "string" ||
    typeof val === "number" ||
    val === null ||
    Array.isArray(val)
  );
};

export const mergeValues = (
  aVal: GenericType,
  bVal: GenericType,
  unique = true,
  sort = false,
): GenericType => {
  if (bVal === undefined) return aVal;
  if (aVal === undefined) return bVal;

  if (!isMergeableValue(aVal) || !isMergeableValue(bVal)) {
    throw new MergeError(
      "mergeValues is not well defined between these types of values",
    );
  }

  const appended = Array.isArray(aVal)
    ? Array.isArray(bVal)
      ? [...aVal, ...bVal]
      : [...aVal, bVal]
    : Array.isArray(bVal)
      ? [aVal, ...bVal]
      : [aVal, bVal];

  const maybeMerged = unique ? Array.from(new Set(appended)) : appended;
  const maybeSorted = sort ? maybeMerged.sort() : maybeMerged;
  return maybeSorted;
};

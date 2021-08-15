import { GenericObject } from "../GenericObject";
import jClone from "../utils/jClone";
import isPlainObject from "lodash.isplainobject";
import { MergeError } from "../errors";

export type UpdateStrategyNames =
  | "useOld"
  | "useNew"
  | "preferOld"
  | "preferNew"
  | "intersection"
  | "removeConflicting"
  | "xor"
  | "merge"
  | "append";

export const updateStrategies: Record<UpdateStrategyNames, CombiningType> = {
  useOld: { justA: true, justB: false, same: true, conflict: "A" },
  useNew: { justA: false, justB: true, same: true, conflict: "B" },
  preferOld: { justA: true, justB: true, same: true, conflict: "A" },
  preferNew: { justA: true, justB: true, same: true, conflict: "B" },
  intersection: { justA: false, justB: false, same: true, conflict: false },
  removeConflicting: { justA: true, justB: true, same: true, conflict: false },
  xor: { justA: true, justB: true, same: false, conflict: false },
  merge: { justA: true, justB: true, same: true, conflict: "merge" },
  append: { justA: true, justB: true, same: true, conflict: "append" },
};

export type conflictStrategies = "A" | "B" | "merge" | "append" | false;

type CombiningType = {
  justA: boolean;
  justB: boolean;
  same: boolean;
  conflict: conflictStrategies;
};

const combineData = (
  aData: GenericObject,
  bData: GenericObject,
  how: UpdateStrategyNames | CombiningType
): GenericObject => {
  const strategy = typeof how === "string" ? updateStrategies[how] : how;
  const aClone = jClone(aData);
  const bClone = jClone(bData);
  const combined = {} as GenericObject;

  for (const key in aClone) {
    const aVal = aClone[key];

    // Just in aData
    if (!(key in bClone)) {
      if (strategy.justA) combined[key] = aVal;
      continue;
    }
    const bVal = bClone[key];
    delete bClone[key];

    // If objects in both, then recurse and run strategy on sub object
    if (isPlainObject(aVal) && isPlainObject(bVal)) {
      combined[key] = combineData(aVal, bVal, how);
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

  return combined;
};

const isMergeableValue = (val: any) => {
  return (
    typeof val === "string" ||
    typeof val === "number" ||
    val === null ||
    Array.isArray(val)
  );
};

// eslint-disable-next-line @typescript-eslint/explicit-module-boundary-types
export const mergeValues = (aVal: any, bVal: any, unique = true): any => {
  if (bVal === undefined) return aVal;
  if (aVal === undefined) return bVal;

  if (!isMergeableValue(aVal) || !isMergeableValue(bVal)) {
    throw new MergeError(
      "mergeValues is not well defined between these types of values"
    );
  }

  const appended = Array.isArray(aVal)
    ? Array.isArray(bVal)
      ? [...aVal, ...bVal]
      : [...aVal, bVal]
    : Array.isArray(bVal)
    ? [aVal, ...bVal]
    : [aVal, bVal];

  if (unique) {
    return Array.from(new Set(appended));
  } else {
    return appended;
  }
};

export default combineData;

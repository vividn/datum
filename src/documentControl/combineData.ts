import { GenericObject } from "../GenericObject";
import jClone from "../utils/jClone";
import isPlainObject from "lodash.isplainobject";

export type UpdateStrategyNames =
  | "useOld"
  | "useNew"
  | "preferOld"
  | "preferNew"
  | "intersection"
  | "removeConflicting"
  | "xor"
  | "merge";
const updateStrategies: Record<UpdateStrategyNames, CombiningType> = {
  useOld: { justA: true, justB: false, same: true, conflict: "A" },
  useNew: { justA: false, justB: true, same: true, conflict: "B" },
  preferOld: { justA: true, justB: true, same: true, conflict: "A" },
  preferNew: { justA: true, justB: true, same: true, conflict: "B" },
  intersection: { justA: false, justB: false, same: true, conflict: false },
  removeConflicting: { justA: true, justB: true, same: true, conflict: false },
  xor: { justA: true, justB: true, same: false, conflict: false },
  merge: { justA: true, justB: true, same: true, conflict: "merge" },
};
export type conflictStrategies = "A" | "B" | "merge" | false;
type CombiningType = {
  justA: boolean;
  justB: boolean;
  same: boolean;
  conflict: conflictStrategies;
};
export const combineData = (
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
      // pass
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

export const mergeValues = (aVal: any, bVal: any, unique=true): any => {

}
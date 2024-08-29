import isPlainObject from "lodash.isplainobject";
import { MyError } from "../errors";
import { JsonObject } from "./utilityTypes";

export class RekeyError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, RekeyError.prototype);
  }
}

export function rekey(aData: JsonObject, bData: JsonObject): JsonObject {
  const newData: JsonObject = {};

  for (const key in aData) {
    if (!Object.keys(bData).includes(key)) {
      // Renaming should clobber existing keys, so if newData already has it don't overwrite
      newData[key] ??= aData[key];
      continue;
    }
    if (isPlainObject(aData[key]) && isPlainObject(bData[key])) {
      newData[key] = rekey(aData[key] as JsonObject, bData[key] as JsonObject);
      continue;
    }

    if (typeof bData[key] !== "string") {
      throw new RekeyError("rekey values must be strings");
    }

    newData[bData[key]] = aData[key];
  }

  return newData;
}

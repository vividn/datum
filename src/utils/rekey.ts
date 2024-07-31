import isPlainObject from "lodash.isplainobject";
import { GenericObject } from "../GenericObject";
import { MyError } from "../errors";

export class RekeyError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, RekeyError.prototype);
  }
}

export function rekey(
  aData: GenericObject,
  bData: GenericObject,
): GenericObject {
  const newData: GenericObject = {};

  for (const key in aData) {
    if (!Object.keys(bData).includes(key)) {
      // Renaming should clobber existing keys, so if newData already has it don't overwrite
      newData[key] ??= aData[key];
      continue;
    }
    if (isPlainObject(aData[key]) && isPlainObject(bData[key])) {
      newData[key] = rekey(aData[key], bData[key]);
      continue;
    }

    if (typeof bData[key] !== "string") {
      throw new RekeyError("rekey values must be strings");
    }

    newData[bData[key]] = aData[key];
  }

  return newData;
}

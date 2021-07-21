import { GenericObject } from "./GenericObject";
import { DataError } from "./errors";
import inferType from "./utils/inferType";
import { splitFirst } from "./utils/splitFirst";
import { createOrAppend } from "./utils/createOrAppend";
import { DatumData } from "./documentControl/DatumDocument";

export type parseDataType = {
  argData: (string | number)[];
  required?: string | string[];
  optional?: string | string[];
  remainder?: string;
  stringRemainder?: boolean;
  field?: string | string[];
  comment?: string | string[];
  lenient?: boolean;
  baseData?: GenericObject;
};
export const parseData = function ({
  argData,
  required = [],
  optional = [],
  remainder,
  stringRemainder,
  field,
  comment,
  lenient = false,
  baseData = {},
}: parseDataType): DatumData {
  const requiredKeys = typeof required === "string" ? [required] : required;
  const optionalKeys = typeof optional === "string" ? [optional] : optional;
  const remainderKey = remainder ?? (lenient ? "extraData" : undefined);
  const remainderData = [];

  const data: DatumData = baseData;

  posArgsLoop: for (const arg of argData) {
    const [first, rest] = splitFirst("=", String(arg));

    if (rest !== undefined) {
      // explicit key is given e.g., 'key=value'
      data[first] = inferType(rest);
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = first;

    requiredKeysLoop: while (requiredKeys.length > 0) {
      const dataKey = requiredKeys.shift()!;

      if (dataKey in data) {
        continue requiredKeysLoop;
      }

      data[dataKey] = inferType(dataValue);
      continue posArgsLoop;
    }

    optionalKeysLoop: while (optionalKeys.length > 0) {
      const [dataKey] = splitFirst("=", optionalKeys.shift()!);

      if (dataKey in data) {
        continue optionalKeysLoop;
      }

      data[dataKey] = inferType(dataValue);
      continue posArgsLoop;
    }

    remainderData.push(dataValue);
  }

  if (remainderData.length > 0) {
    if (remainderKey === undefined) {
      throw new DataError(
        "some data do not have keys. Assign keys with equals signs, use required/optional keys, specify a key to use as --remainder, or use --lenient"
      );
    }

    if (stringRemainder) {
      data[remainderKey] = createOrAppend(
        data[remainderKey],
        remainderData.join(" ")
      );
    } else {
      for (const remainder of remainderData) {
        data[remainderKey] = createOrAppend(
          data[remainderKey],
          inferType(remainder)
        );
      }
    }
  }

  if (requiredKeys.length > 0) {
    throw new DataError(
      `No data given for the required key(s) '${requiredKeys}`
    );
  }

  // If extra keys are left assign default values
  while (optionalKeys.length > 0) {
    const [dataKey, defaultValue] = splitFirst("=", optionalKeys.shift()!);

    if (dataKey in data || defaultValue === undefined) {
      continue;
    }

    data[dataKey] = inferType(defaultValue);
  }

  // put in field, overwriting if necessary
  if (field) {
    const rawValue = typeof field === "string" ? field : field.slice(-1)[0];
    data.field = inferType(rawValue);
  }

  if (comment) {
    const inferredComments = (Array.isArray(comment)
      ? comment.map((comm) => inferType(comm))
      : [inferType(comment)]) as any[];
    data.comment = inferredComments.reduce(
      (accumulator, current) => createOrAppend(accumulator, current),
      data["comment"]
    );
  }

  return data;
};
// TODO: Keys that end in -Time or -Date should be parsed as such

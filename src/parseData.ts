import { BaseDataError, DataError } from "./errors";
import inferType from "./utils/inferType";
import { splitFirst } from "./utils/splitFirst";
import { createOrAppend } from "./utils/createOrAppend";
import { DatumData } from "./documentControl/DatumDocument";
import { DataInputArgs } from "./input/dataArgs";

export type ParseDataType = DataInputArgs;
export const parseData = function ({
  data = [],
  required = [],
  optional = [],
  remainder,
  stringRemainder,
  field,
  comment,
  lenient = false,
  baseData,
}: ParseDataType): DatumData {
  const requiredKeys = typeof required === "string" ? [required] : required;
  const optionalKeys = typeof optional === "string" ? [optional] : optional;
  const remainderKey = remainder ?? (lenient ? "extraData" : undefined);
  const remainderData = [];

  const parsedData: DatumData = baseData ? inferType(baseData) : {};
  if (typeof parsedData !== "object" || parsedData === null) {
    throw new BaseDataError("base data not a valid object");
  }

  posArgsLoop: for (const arg of data) {
    const [beforeEquals, afterEquals] = splitFirst("=", String(arg));

    if (afterEquals !== undefined) {
      // explicit key is given e.g., 'key=value'
      parsedData[beforeEquals] = inferType(afterEquals);
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = beforeEquals;

    requiredKeysLoop: while (requiredKeys.length > 0) {
      const dataKey = requiredKeys.shift()!;

      if (dataKey in parsedData) {
        continue requiredKeysLoop;
      }

      parsedData[dataKey] = inferType(dataValue);
      continue posArgsLoop;
    }

    optionalKeysLoop: while (optionalKeys.length > 0) {
      const [dataKey] = splitFirst("=", optionalKeys.shift()!);

      if (dataKey in parsedData) {
        continue optionalKeysLoop;
      }

      parsedData[dataKey] = inferType(dataValue);
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
      parsedData[remainderKey] = createOrAppend(
        parsedData[remainderKey],
        remainderData.join(" ")
      );
    } else {
      for (const remainder of remainderData) {
        parsedData[remainderKey] = createOrAppend(
          parsedData[remainderKey],
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

    if (dataKey in parsedData || defaultValue === undefined) {
      continue;
    }

    parsedData[dataKey] = inferType(defaultValue);
  }

  // put in field, overwriting if necessary
  if (field) {
    parsedData.field = inferType(field);
  }

  if (comment) {
    const inferredComments = (
      Array.isArray(comment)
        ? comment.map((comm) => inferType(comm))
        : [inferType(comment)]
    ) as any[];
    parsedData.comment = inferredComments.reduce(
      (accumulator, current) => createOrAppend(accumulator, current),
      parsedData["comment"]
    );
  }

  return parsedData;
};
// TODO: Keys that end in -Time or -Date should be parsed as such

import { GenericObject } from "./GenericObject";
import { DataError } from "./errors";
import inferType from "./utils/inferType";
import { splitFirstEquals } from "./utils/splitFirstEquals";
import { createOrAppend } from "./utils/createOrAppend";

type parseDataType = {
  posArgs: (string | number)[];
  required?: string | string[];
  optional?: string | string[];
  remainder?: string;
  stringRemainder?: boolean;
  field?: string | string[];
  comment?: string | string[];
  lenient?: boolean;
  payload?: GenericObject;
};
export const parseData = function ({
  posArgs,
  required = [],
  optional = [],
  remainder,
  stringRemainder,
  field,
  comment,
  lenient = false,
  payload = {},
}: parseDataType): GenericObject {
  const requiredKeys = typeof required === "string" ? [required] : required;
  const optionalKeys = typeof optional === "string" ? [optional] : optional;
  const remainderKey = remainder ?? (lenient ? "extraData" : undefined);
  const remainderData = [];

  posArgsLoop: for (const arg of posArgs) {
    const [first, rest] = splitFirstEquals(String(arg));

    if (rest !== undefined) {
      // explicit key is given e.g., 'key=value'
      payload[first] = inferType(rest);
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = first;

    requiredKeysLoop: while (requiredKeys.length > 0) {
      const dataKey = requiredKeys.shift()!;

      if (dataKey in payload) {
        continue requiredKeysLoop;
      }

      payload[dataKey] = inferType(dataValue);
      continue posArgsLoop;
    }

    optionalKeysLoop: while (optionalKeys.length > 0) {
      const [dataKey, defaultValue] = splitFirstEquals(optionalKeys.shift()!);

      if (dataKey in payload) {
        continue optionalKeysLoop;
      }

      payload[dataKey] = inferType(dataValue);
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
      payload[remainderKey] = createOrAppend(
        payload[remainderKey],
        remainderData.join(" ")
      );
    } else {
      for (const remainder of remainderData) {
        payload[remainderKey] = createOrAppend(
          payload[remainderKey],
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
    const [dataKey, defaultValue] = splitFirstEquals(optionalKeys.shift()!);

    if (dataKey in payload || defaultValue === undefined) {
      continue;
    }

    payload[dataKey] = inferType(defaultValue);
  }

  // put in field, overwriting if necessary
  if (field) {
    const rawValue = typeof field === "string" ? field : field.slice(-1)[0];
    payload.field = inferType(rawValue);
  }

  if (comment) {
    const inferredComments = (Array.isArray(comment)
      ? comment.map((comm) => inferType(comm))
      : [inferType(comment)]) as any[];
    payload.comment = inferredComments.reduce(
      (accumulator, current) => createOrAppend(accumulator, current),
      payload["comment"]
    );
  }

  return payload;
};

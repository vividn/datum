import { inferType } from "./utils";
import { GenericObject } from "./types";
const utils = require("./utils");
const { DataError } = require("./errors");

type parseDataType = {
  posArgs: (string | number)[];
  required?: string | string[];
  optional?: string | string[];
  remainder?: string;
  field?: string | string[];
  comment?: string | string[];
  lenient?: boolean;
  payload?: GenericObject;
};
const parseData = function ({
  posArgs,
  required = [],
  optional = [],
  remainder,
  field,
  comment,
  lenient = false,
  payload = {},
}: parseDataType): GenericObject {
  const requiredKeys = typeof required === "string" ? [required] : required;
  const optionalKeys = typeof optional === "string" ? [optional] : optional;
  const remainderKey = remainder ?? (lenient ? "extraData" : undefined);

  posArgsLoop: for (const arg of posArgs) {
    const [first, rest] = utils.splitFirstEquals(String(arg));

    if (rest !== undefined) {
      // explicit key is given e.g., 'key=value'
      payload[first] = utils.inferType(rest);
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = first;

    requiredKeysLoop: while (requiredKeys.length > 0) {
      const dataKey = requiredKeys.shift()!;

      if (dataKey in payload) {
        continue requiredKeysLoop;
      }

      payload[dataKey] = utils.inferType(dataValue);
      continue posArgsLoop;
    }

    optionalKeysLoop: while (optionalKeys.length > 0) {
      const [dataKey, defaultValue] = utils.splitFirstEquals(
        optionalKeys.shift()!
      );

      if (dataKey in payload) {
        continue optionalKeysLoop;
      }

      payload[dataKey] = utils.inferType(dataValue);
      continue posArgsLoop;
    }

    // data remains, but no extraKeys left
    if (remainderKey !== undefined) {
      payload[remainderKey] = utils.createOrAppend(
        payload[remainderKey],
        utils.inferType(dataValue)
      );
      continue;
    }
    throw new DataError(
      "some data do not have keys. Assign keys with equals signs, use required/optional keys, specify a key to use as --remainder, or use --lenient"
    );
  }

  if (requiredKeys.length > 0) {
    throw new DataError(
      `No data given for the required key(s) '${requiredKeys}`
    );
  }

  // If extraKeys are left assign default values
  while (optionalKeys.length > 0) {
    const [dataKey, defaultValue] = utils.splitFirstEquals(
      optionalKeys.shift()!
    );

    if (dataKey in payload || defaultValue === undefined) {
      continue;
    }

    payload[dataKey] = utils.inferType(defaultValue);
  }

  // put in field, overwriting if necessary
  if (field) {
    const rawValue = typeof field === "string" ? field : field.slice(-1)[0];
    payload.field = utils.inferType(rawValue);
  }

  if (comment) {
    const inferredComments = (Array.isArray(comment)
      ? comment.map((comm) => utils.inferType(comm))
      : [utils.inferType(comment)]) as any[];
    payload.comment = inferredComments.reduce(
      (accumulator, current) => utils.createOrAppend(accumulator, current),
      payload["comment"]
    );
  }

  return payload;
};

module.exports = {
  parseData,
};

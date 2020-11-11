const utils = require("./utils");

type parseDataType = {
  posArgs: (string | number)[];
  extraKeys?: string | string[];
  field?: string | string[];
  comment?: string | string[];
  lenient?: boolean;
  payload?: { [key: string]: any };
};
const parseData = function ({
  posArgs,
  extraKeys = [],
  field,
  comment,
  lenient = false,
  payload = {},
}: parseDataType): { [key: string]: any } {
  const keyArray = typeof extraKeys === "string" ? [extraKeys] : extraKeys;
  let hasEncounteredOptionalKey = false;

  posArgsLoop: for (const arg of posArgs) {
    const [first, rest] = utils.splitFirstEquals(String(arg));

    if (rest !== undefined) {
      // explicit key is given e.g., 'key=value'
      payload[first] = utils.inferType(rest);
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = first;

    extraKeyLoop: while (keyArray.length > 0) {
      const [dataKey, defaultValue] = utils.splitFirstEquals(keyArray.shift()!);

      // All required arguments must come before optional argument
      if (defaultValue === undefined) {
        if (hasEncounteredOptionalKey) {
          throw new KeysError(
            "All required extra keys must come before all optional keys"
          ); // TODO: sort required extraKeys first
        }
      } else {
        hasEncounteredOptionalKey = true;
      }

      // Skip to next extraKey if the value has already been explicityly provided
      if (dataKey in payload) {
        continue extraKeyLoop;
      }

      payload[dataKey] = utils.inferType(dataValue);
      continue posArgsLoop;
    }

    // data remains, but no extraKeys left
    if (lenient) {
      payload.extraData = (payload.extraData || []).concat([
        utils.inferType(dataValue),
      ]);
      continue;
    }
    throw new DataError(
      "some data do not have keys. Assign keys with equals signs `key=value`, key args `-K key1 -K key2 value1 value2`, or use --lenient"
    );
  }

  // If extraKeys are left assign default values
  while (keyArray.length > 0) {
    const [dataKey, defaultValue] = utils.splitFirstEquals(keyArray.shift()!);

    if (defaultValue === undefined && hasEncounteredOptionalKey) {
      throw new KeysError(
        "All required extra keys must come before all optional keys"
      ); // TODO: sort required extraKeys first
    }

    if (dataKey in payload) {
      continue;
    }

    if (defaultValue === undefined) {
      throw new DataError(`No data given for the required key '${dataKey}`);
    }

    if (defaultValue === "") {
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
    if ("comment" in payload) {
      payload.comment = [payload.comment].concat(utils.inferType(comment));
    } else {
      payload.comment = utils.inferType(comment);
    }
  }

  return payload;
};

class DataError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

class KeysError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

module.exports = {
  parseData,
  DataError,
  KeysError,
};

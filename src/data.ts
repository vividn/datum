type parseDataType = {
  posArgs: (string | number)[];
  extraKeys?: string | string[];
  lenient?: boolean;
};
const parseData = function ({
  posArgs,
  extraKeys,
  lenient = false,
}: parseDataType) {
  const payload: { [key: string]: any } = {};

  // split into arguments that come with a key "key=value", and those without "value"
  const [withKey, withoutKey] = posArgs.reduce(
    (result, element) => {
      if (typeof element === "string" && element.includes("=")) {
        result[0].push(element);
      } else {
        result[1].push(element);
      }
      return result;
    },
    [[] as string[], [] as (string | number)[]]
  );

  for (const arg of withKey) {
    const [key, ...eqSepValue] = arg.split("=");
    payload[key] = inferType(eqSepValue.join('='));
  }

  if (withoutKey.length > 0) {
    if (lenient) {
      payload["extraData"] = withoutKey.map((arg) => {
        return inferType(arg)});
    } else {
      throw new DataError('some data do not have keys. Assign keys with equals signs `key=value`, key args `-K key1 -K key2 value1 value2`, or use --lenient');
    }
  }

  return payload;
};

const inferType = (value: (number | string)) => {
  if (typeof(value) === "number") {
    return value
  }
  if(/^null$/i.test(value)) {
    return null
  }
  if(/^nan$/i.test(value)) {
    return Number.NaN
  }
  if(/^-?inf(inity)?/i.test(value)) {
    return value[0] === '-' ? Number.NEGATIVE_INFINITY : Number.POSITIVE_INFINITY
  }
  try {
    const RJSON = require("relaxed-json");
    return RJSON.parse(value);
  } catch {}
  return value;
}

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

module.exports = { parseData, DataError, inferType, KeysError };

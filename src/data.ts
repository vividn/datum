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

  // save the arguments with explicit keys into the payload leaving only the keyless
  const withoutKey = posArgs.reduce((result, arg) => {
    if (typeof arg === "string" && arg.includes("=")) {
      const [key, ...eqSepValue] = arg.split("=");
      payload[key] = inferType(eqSepValue.join("="));
      return result;
    }
    result.push(arg);
    return result;
  }, [] as (string | number)[]);

  if (withoutKey.length > 0) {
    if (lenient) {
      payload["extraData"] = withoutKey.map((arg) => {
        return inferType(arg);
      });
    } else {
      throw new DataError(
        "some data do not have keys. Assign keys with equals signs `key=value`, key args `-K key1 -K key2 value1 value2`, or use --lenient"
      );
    }
  }

  return payload;
};

const inferType = (value: number | string) => {
  if (typeof value === "number") {
    return value;
  }
  if (/^null$/i.test(value)) {
    return null;
  }
  if (/^nan$/i.test(value)) {
    return Number.NaN;
  }
  if (/^-?inf(inity)?/i.test(value)) {
    return value[0] === "-"
      ? Number.NEGATIVE_INFINITY
      : Number.POSITIVE_INFINITY;
  }
  try {
    const RJSON = require("relaxed-json");
    return RJSON.parse(value);
  } catch {}
  return value;
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

module.exports = { parseData, DataError, inferType, KeysError };

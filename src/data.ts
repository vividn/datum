type parseDataType = {
  posArgs: (string | number)[];
  extraKeys?: string | string[];
  lenient?: boolean;
  payload?: {[key: string]: any}
};
const parseData = function ({
  posArgs,
  extraKeys,
  lenient = false,
  payload = {},
}: parseDataType) {

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

const combineExtraKeysWithArgs = function ({posArgs, extraKeys = [], lenient, payload = {}}: parseDataType) {
  for (const extraKey of extraKeys) {
    const [dataKey, defaultValue, tooManyEquals] = extraKey.split('=');
    if (tooManyEquals !== undefined) {
      throw 'Too many equals signs in a key in --extra-keys';
    }

    // the data key might be already explicityly specified
    if (dataKey in payload) {
      continue;
    }

    const positionalValue = withoutKey.shift();
    if (defaultValue === undefined) {
      if (noMoreRequiredPositionals) {
        throw 'All required extra keys must come before all optional keys';
      }
      if (positionalValue === undefined) {
        throw `No data given for the required key '${dataKey}`;
      }
    }
    if (defaultValue !== undefined) {
      noMoreRequiredPositionals = true;
    }
    // default value is '' when nothing is given after the =
    payload[dataKey] =
      positionalValue ??
      (defaultValue === '' ? undefined : Number(defaultValue) || defaultValue);
  }
}

const splitFirstEquals = (str: string): [string, string | null] => {
  const [first, ...eqSepValue] = str.split("=");
  if (eqSepValue.length === 0) {
    return [first, null]
  }
  return [first, eqSepValue.join("=")]
}

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

module.exports = { parseData, DataError, inferType, splitFirstEquals, KeysError };

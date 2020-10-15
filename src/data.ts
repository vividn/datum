const RJSON = require("relaxed-json");

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

  // Processing the args with keys is easy enough
  for (const arg of withKey) {
    const [key, value] = arg.split("=");
    payload[key] = RJSON.parse(value);
  }

  if (withoutKey.length > 0) {
    if (lenient) {
      payload["extraData"] = withoutKey.map((arg) => {
        console.log(arg)
        return RJSON.parse(String(arg))});
    } else {
      throw new DataError('some data do not have keys. Assign keys with equals signs `key=value`, key args `-K key1 -K key2 value1 value2`, or use --lenient');
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

module.exports = { parseData, DataError };

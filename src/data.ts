const RJSON = require('relaxed-json')

type parseDataType = {
  posArgs: (string | number)[];
  extraKeys?: string | string[];
};
const parseData = function ({ posArgs, extraKeys }: parseDataType) {
  const payload: {[key: string]: any} = {}

  // split into arguments that come with a key "key=value", and those without "value"
  const [withKey, withoutKey] = posArgs.reduce(
    (result, element) => {
      if (typeof element === 'string' && element.includes('=')) {
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
    const [key, value] = arg.split('=');
    payload[key] = RJSON.parse(value);
  }

  return payload
};

module.exports = { parseData }
export const inferType = (value: number | string) => {
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

export const splitFirstEquals = (str: string): [string, string | undefined] => {
  const [first, ...eqSepValue] = str.split("=");
  if (eqSepValue.length === 0) {
    return [first, undefined];
  }
  return [first, eqSepValue.join("=")];
};

const createOrAppend = (existing: any, toAppend: any ): any => {
  if ( existing === undefined ) {
    return toAppend
  }
  const existingArr = (Array.isArray(existing ?? []) ? (existing ?? []) : [existing])
  existingArr.push(toAppend)
  return existingArr
}

module.exports = { inferType, splitFirstEquals, createOrAppend };

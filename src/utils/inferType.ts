import RJSON from "relaxed-json";

const inferType = (value: number | string): any => {
  if (typeof value === "number") {
    return value;
  }
  if (/^null$/i.test(value)) {
    return null;
  }
  if (/^nan$/i.test(value)) {
    return Number.NaN;
  }
  if (/^-?inf(inity)?$/i.test(value)) {
    return value[0] === "-"
      ? Number.NEGATIVE_INFINITY
      : Number.POSITIVE_INFINITY;
  }
  try {
    return RJSON.parse(value);
  } catch {
    // If RJSON can't parse than just return the initial value
  }
  return value;
};

export default inferType;

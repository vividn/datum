import { HIGH_STRING, nextFloat } from "./keyEpsilon";
import { QueryOptions } from "./utilityTypes";

export function startsWith(
  value: string | number | any[] | any,
): Required<Pick<QueryOptions, "startkey" | "endkey">> {
  if (typeof value === "string") {
    return { startkey: value, endkey: value + HIGH_STRING };
  } else if (typeof value === "number") {
    const endKey = nextFloat(value, +Infinity);
    return { startkey: value, endkey: endKey };
  } else if (Array.isArray(value)) {
    const incrementedLastValue = startsWith(value.at(-1)).endkey;
    return {
      startkey: value,
      endkey: [...value.slice(0, -1), incrementedLastValue],
    };
  } else {
    return {
      startkey: value,
      endkey: { [HIGH_STRING]: HIGH_STRING },
    };
  }
}

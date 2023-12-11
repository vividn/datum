import RJSON from "relaxed-json";
import { parseTimeStr } from "../time/parseTimeStr";
import { parseDurationStr } from "../time/parseDurationStr";
import { parseDateStr } from "../time/parseDateStr";
import {
  isoDateFromDateTime,
  isoDurationFromDuration,
  toDatumTime,
} from "../time/timeUtils";

export function inferType(
  value: number | string,
  fieldName?: string,
  defaultToInfer?: string | number,
): any {
  if (value === "") {
    // can use `""` or `''` to indicate an empty string, see further down
    return undefined;
  }
  if (value === ".") {
    if (defaultToInfer === undefined) {
      return undefined;
    }
    return inferType(defaultToInfer, fieldName);
  }

  if (fieldName !== undefined) {
    switch (true) {
      case /(?:\b|_)time\d*$/i.test(fieldName):
      case /[a-z0-9]Time\d*$/.test(fieldName): {
        const parsedTime = parseTimeStr({ timeStr: String(value) });
        return toDatumTime(parsedTime);
      }

      case /(?:\b|_)date\d*$/i.test(fieldName!):
      case /[a-z0-9]Date\d*$/.test(fieldName): {
        const parsedDate = parseDateStr({ dateStr: String(value) });
        return isoDateFromDateTime(parsedDate);
      }

      case /(?:\b|_)dur(ation)?\d*$/i.test(fieldName!):
      case /[a-z0-9]Dur(ation)?\d*$/.test(fieldName): {
        const parsedDuration = parseDurationStr({ durationStr: String(value) });
        return isoDurationFromDuration(parsedDuration);
      }
    }
  }

  if (typeof value === "number") {
    return value;
  }
  if (/^\\.$/.test(value)) {
    return ".";
  }
  if (/^,$/.test(value)) {
    return [];
  }
  if (/^,/.test(value)) {
    return value
      .slice(1)
      .split(",")
      .map((v) => inferType(v));
  }
  if (/,$/.test(value)) {
    return value
      .slice(0, -1)
      .split(",")
      .map((v) => inferType(v));
  }
  if (/^true$/i.test(value)) {
    return true;
  }
  if (/^false$/i.test(value)) {
    return false;
  }
  if (/(^''$)|(^""$)/.test(value)) {
    return "";
  }
  if (/^undefined$/i.test(value)) {
    return undefined;
  }
  if (/^null$/i.test(value)) {
    return null;
  }
  if (/^nan$/i.test(value)) {
    return "NaN";
  }
  if (/^-?inf(inity)?$/i.test(value)) {
    return value[0] === "-" ? "-Infinity" : "Infinity";
  }
  try {
    return RJSON.parse(value);
  } catch {
    // If RJSON can't parse than just return the initial value
  }
  return value;
}

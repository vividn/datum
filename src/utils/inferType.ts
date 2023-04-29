import RJSON from "relaxed-json";
import { parseTimeStr } from "../time/parseTimeStr";
import { BadDateError, BadDurationError, BadTimeError } from "../errors";
import { parseDurationStr } from "../time/parseDurationString";
import { parseDateStr } from "../time/parseDateStr";
import {
  isoDateFromDateTime,
  isoDatetimeFromDateTime,
  isoDurationFromDuration,
} from "../time/timeUtils";

export function inferType(value: number | string, fieldName?: string): any {
  if (fieldName !== undefined) {
    switch (true) {
      case /(?:\b|_)time$/i.test(fieldName):
      case /[a-z0-9]Time$/.test(fieldName):
        try {
          const parsedTime = parseTimeStr({ timeStr: String(value) });
          return isoDatetimeFromDateTime(parsedTime);
        } catch (e) {
          if (e instanceof BadTimeError) {
            // pass
          } else {
            throw e;
          }
        }
        break;

      case /(?:\b|_)date$/i.test(fieldName!):
      case /[a-z0-9]Date$/.test(fieldName):
        try {
          const parsedDate = parseDateStr({ dateStr: String(value) });
          return isoDateFromDateTime(parsedDate);
        } catch (e) {
          if (e instanceof BadDateError) {
            // pass
          } else {
            throw e;
          }
        }
        break;

      case /(?:\b|_)dur(ation)?$/i.test(fieldName!):
      case /[a-z0-9]Dur(ation)?$/.test(fieldName):
        try {
          const parsedDuration = parseDurationStr({
            durationStr: String(value),
          });
          return isoDurationFromDuration(parsedDuration);
        } catch (e) {
          if (e instanceof BadDurationError) {
            // pass
          } else {
            throw e;
          }
        }
        break;
    }
  }

  if (typeof value === "number") {
    return value;
  }
  if (/^,/.test(value)) {
    return value.slice(1).split(",").map((v) => inferType(v));
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

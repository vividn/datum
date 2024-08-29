import isPlainObject from "lodash.isplainobject";
import { DateTime } from "luxon";
import { BadTimeError } from "../errors";
import { JsonObject, JsonType } from "../utils/utilityTypes";
import { getTimezone } from "./getTimezone";
import { parseTimeStr } from "./parseTimeStr";
import {
  isIsoDateOrTime,
  isoDateFromDateTime,
  isoDateOrTime,
  isoDatetimeFromDateTime,
  utcOffset,
} from "./timeUtils";

type IANATimeZone = string;
export type DatumTime = {
  utc: isoDateOrTime;
  o?: number;
  tz?: IANATimeZone;
};

export function toDatumTime(
  time: DateTime | string,
  onlyDate?: boolean,
): DatumTime {
  if (typeof time === "string") {
    time = parseTimeStr({ timeStr: time });
  }
  // Checking if DateTime is valid should be done before calling this function
  if (!time.isValid) {
    throw new BadTimeError();
  }
  if (onlyDate) {
    return { utc: isoDateFromDateTime(time) };
  }
  return {
    utc: isoDatetimeFromDateTime(time),
    o: utcOffset(time),
    tz: time.zone.name,
  };
}

export function datumTimeToLuxon(time?: DatumTime): DateTime | undefined {
  // returns undefined if utc is just a date or some invalid string
  if (
    time === undefined ||
    !isIsoDateOrTime(time.utc) ||
    !time.utc.includes("T")
  ) {
    return undefined;
  }
  if (time.tz && time.o) {
    const tzTime = DateTime.fromISO(time.utc, { zone: getTimezone(time.tz) });
    const oTime = DateTime.fromISO(time.utc, { zone: getTimezone(time.o) });
    if (tzTime.offset !== oTime.offset) {
      console.warn(
        `mismatched IANA timezone and offset for ${time.utc}. zone = ${time.tz}, o = ${time.o}`,
      );
      return oTime;
    }
  }
  const timezone = time.tz
    ? getTimezone(time.tz)
    : time.o
      ? getTimezone(time.o)
      : undefined;
  const dateTime = DateTime.fromISO(time.utc, {
    zone: timezone,
  });
  return dateTime.isValid ? dateTime : undefined;
}

export function isDatumTime(
  time: DateTime | DatumTime | JsonObject | JsonType,
): time is DatumTime {
  if (!isPlainObject(time)) {
    return false;
  }
  time = time as DatumTime;
  return (
    typeof time.utc === "string" &&
    isIsoDateOrTime(time.utc) &&
    (time.o === undefined || typeof time.o === "number") &&
    (time.tz === undefined || typeof time.tz === "string")
  );
}

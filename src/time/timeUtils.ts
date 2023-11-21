import { DateTime, Duration, Zone } from "luxon";
import { BadDurationError } from "../errors";
import { GenericObject } from "../GenericObject";

export type isoDatetime = string;
export type isoDate = string;
export type isoDateOrTime = isoDate | isoDatetime;
export type isoDuration = string;

type IANATimeZone = string;
export type DatumTime = {
  utc: isoDateOrTime;
  o?: number;
  tz?: IANATimeZone;
};

export function isIsoDateOrTime(str: string): str is isoDateOrTime {
  return DateTime.fromISO(str).isValid;
}

export function isDatumTime(
  time: DatumTime | GenericObject | string
): time is DatumTime {
  if (typeof time === "string") {
    return false;
  }
  return (
    typeof time.utc === "string" &&
    isIsoDateOrTime(time.utc) &&
    (time.o === undefined || typeof time.o === "number") &&
    (time.tz === undefined || typeof time.tz === "string")
  );
}

export const now = (zone?: Zone | string): DateTime => DateTime.local({ zone });

export function isoDateFromDateTime(dt: DateTime): isoDate {
  return dt.toISODate() as isoDate;
}

export function isoDatetimeFromDateTime(dt: DateTime): isoDatetime {
  return dt.toUTC().toString() as isoDatetime;
}

export function isoDurationFromDuration(dur: Duration): isoDuration {
  if (dur.valueOf() < 0) {
    return "-" + dur.negate().toISO();
  }
  if (!dur.isValid) {
    throw new BadDurationError("invalid duration");
  }
  return dur.toISO() as isoDuration;
}

export function utcOffset(referenceTime: DateTime): number {
  const offset = referenceTime.offset / 60;
  // luxon sometimes gives -0 for the offset if system timezone is utc, so make it positive if necessary
  return Object.is(offset, -0) ? 0 : offset;
}

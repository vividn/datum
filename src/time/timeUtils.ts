import { DateTime, Duration, Zone, Settings as DateTimeSettings } from "luxon";
import { BadDurationError } from "../errors";

export type isoDatetime = string;
export type isoDate = string;
export type isoDateOrTime = isoDate | isoDatetime;
export type isoDuration = string;

export function isIsoDateOrTime(str: string): str is isoDateOrTime {
  return DateTime.fromISO(str).isValid;
}

export const now = (zone?: Zone | string): DateTime => DateTime.local({ zone });
export const today = (zone?: Zone | string): isoDate | null =>
  now(zone).toISODate();
export const defaultZone = DateTimeSettings.defaultZone as Zone;

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
    throw new BadDurationError();
  }
  return dur.toISO() as isoDuration;
}

export function utcOffset(time: DateTime): number {
  const offset = time.offset / 60;
  // luxon sometimes gives -0 for the offset if system timezone is utc, so make it positive if necessary
  return Object.is(offset, -0) ? 0 : offset;
}

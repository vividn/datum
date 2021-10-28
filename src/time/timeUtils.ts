import { DateTime, Duration } from "luxon";

export type isoDatetime = string;
export type isoDate = string;
export type isoDuration = string;

export function isIsoDateOrTime(str: string): str is isoDate | isoDatetime {
  return DateTime.fromISO(str).isValid;
}

export const now = DateTime.local;

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
  return dur.toISO();
}

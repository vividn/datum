import { DateTime, Duration, Zone } from "luxon";

export type isoDatetime = string;
export type isoDate = string;
export type isoDuration = string;

export function isIsoDateOrTime(str: string): str is isoDate | isoDatetime {
  return DateTime.fromISO(str).isValid;
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
  return dur.toISO();
}

export function utcOffset(referenceTime: DateTime): number {
  const offset = referenceTime.offset / 60;
  // luxon sometimes gives -0 for the offset if system timezone is utc, so make it positive if necessary
  return Object.is(offset, -0) ? 0 : offset;
}

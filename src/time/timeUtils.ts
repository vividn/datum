import { DateTime, Duration, Zone, Settings as DateTimeSettings } from "luxon";
import { BadDurationError, BadTimeError } from "../errors";
import { GenericObject } from "../GenericObject";
import { getTimezone } from "./getTimezone";

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
    throw new BadDurationError("invalid duration");
  }
  return dur.toISO() as isoDuration;
}

export function utcOffset(time: DateTime): number {
  const offset = time.offset / 60;
  // luxon sometimes gives -0 for the offset if system timezone is utc, so make it positive if necessary
  return Object.is(offset, -0) ? 0 : offset;
}

export function toDatumTime(time: DateTime | string, onlyDate?: boolean): DatumTime {
  if (typeof time === "string") {
    time = DateTime.fromISO(time);
  }
  // Checking if DateTime is valid should be done before calling this function
  if (!time.isValid) {
    throw new BadTimeError("invalid time was given");
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
        `mismatched IANA timezone and offset for ${time.utc}. zone = ${time.tz}, o = ${time.o}`
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

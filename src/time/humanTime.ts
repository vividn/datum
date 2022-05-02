import { DateTime, Duration, FixedOffsetZone } from "luxon";
import { isoDatetime } from "./timeUtils";

export function humanTime(time: DateTime): string {
  let dateTimeFormat;
  const now = DateTime.now();
  const yesterday = now.minus(Duration.fromISO("P1D"));
  const tomorrow = now.plus(Duration.fromISO("P1D"));
  if (now.hasSame(time, "day")) {
    dateTimeFormat = "HH:mm:ss";
  } else if (yesterday.hasSame(time, "day")) {
    dateTimeFormat = "'-1d,' HH:mm:ss";
  } else if (tomorrow.hasSame(time, "day")) {
    dateTimeFormat = "'+1d,' HH:mm:ss";
  } else if (now.hasSame(time, "year")) {
    dateTimeFormat = "MMM d, HH:mm:ss";
  } else {
    dateTimeFormat = "yyyy-MM-dd, HH:mm:ss";
  }

  if (now.offset !== time.offset) {
    dateTimeFormat += " 'UTC'Z";
  }

  return time.toFormat(dateTimeFormat);
}

export function humanTimeFromISO(timeStr: isoDatetime, utcOffset?: number): string {
  const datetime = DateTime.fromISO(timeStr,{
                zone: utcOffset !== undefined
                  ? FixedOffsetZone.instance(60 * utcOffset)
                  : undefined,
              });
  return humanTime(datetime);
}

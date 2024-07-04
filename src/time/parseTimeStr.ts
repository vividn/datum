import { DateTime, Duration } from "luxon";
import { now } from "./timeUtils";
import * as chrono from "chrono-node";
import { BadTimeError } from "../errors";
import { parseDurationStr } from "./parseDurationStr";

type ParseTimeStrType = {
  timeStr: string;
  referenceTime?: DateTime;
};

// TODO: Change this not to use an object input
export function parseTimeStr({
  timeStr,
  referenceTime,
}: ParseTimeStrType): DateTime {
  referenceTime = referenceTime ?? now();

  // Quick relative time strings (q = 5 minutes, t = 1 minute)
  const quickMatches = timeStr.match(/^([+-]?)([qt]+)$/i);
  if (quickMatches) {
    const sign = quickMatches[1] === "+" ? 1 : -1;
    const qs = quickMatches[2].match(/q/gi)?.length ?? 0;
    const ts = quickMatches[2].match(/t/gi)?.length ?? 0;
    const minutes = sign * (qs * 5 + ts);
    const duration = Duration.fromObject({ minutes });
    return referenceTime.plus(duration);
  }

  // This custom regex is to match a few extra strings not recognized by chrono, particularly short
  // E.g, 10 for 10:00, 1513 for 15:13, etc.
  const matches = timeStr.match(
    /^(?<hour>\d{1,2})(:?(?<minute>\d{2})(:?(?<second>\d{2})(\.(?<milli>\d{1,3}))?)?)?(?<meridian>am?|pm?)?$/i,
  );
  if (matches?.groups) {
    const {
      hour = "0",
      minute = "0",
      second = "0",
      milli = "0",
      meridian,
    } = matches.groups;

    // fairly dirty implementation, but built for speed
    const correctedHour =
      meridian?.toLowerCase() === "pm"
        ? parseInt(hour, 10) + 12
        : parseInt(hour, 10);

    // if less than all three millisecond digits are given, fill the remaining zeroes
    const millisecond = parseInt((milli + "000").substring(0, 3), 10);

    return referenceTime.set({
      hour: correctedHour,
      minute: parseInt(minute, 10),
      second: parseInt(second, 10),
      millisecond,
    });
  }

  // Also supports relative time strings, e.g., -5min
  if (timeStr.match(/^[+-]/)) {
    const duration = parseDurationStr({ durationStr: timeStr }) as Duration;
    return referenceTime.plus(duration);
  }

  // DateTime can parse some extra ISO type strings
  const dateTimeParsed = DateTime.fromISO(timeStr, { setZone: true });
  if (dateTimeParsed.isValid) {
    return dateTimeParsed;
  }

  // As a last resort, use chrono to parse the time
  const chronoParsed = chrono.parseDate(timeStr, {
    instant: referenceTime.toJSDate(),
    timezone: referenceTime.zone.offset(referenceTime.toMillis()),
  });
  if (chronoParsed) {
    return DateTime.fromISO(chronoParsed.toISOString(), {
      zone: referenceTime.zone,
    });
  }

  throw new BadTimeError(timeStr);
}

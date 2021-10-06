import { DateTime, Duration } from "luxon";
import { now } from "./timeUtils";
import * as chrono from "chrono-node";
import { BadTimeArgError } from "../errors";

type ParseTimeStrType = {
  timeStr: string;
  referenceTime?: DateTime;
};
export const parseTimeStr = function ({
  timeStr,
  referenceTime,
}: ParseTimeStrType): DateTime {
  referenceTime = referenceTime ?? now();

  // This custom regex is to match a few extra strings not recognized by chrono, particularly short
  // E.g, 10 for 10:00, 1513 for 15:13, etc.
  const matches = timeStr.match(
    /^(?<hour>\d{1,2})(:?(?<minute>\d{2})(:?(?<second>\d{2})(\.(?<milli>\d{1,3}))?)?)?(?<meridian>am?|pm?)?$/i
  );
  if (matches?.groups) {
    const {
      hour = "0",
      minute = "0",
      second = "0",
      milli = "0",
      meridian,
    } = matches.groups;
    const correctedHour = // fairly dirty implementation, but built for speed
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
  const relTimeMatches = timeStr.match(
    /^(?<sign>[+-])(?<value>\d+(\.\d)?) ?(?<units>s|secs?|seconds?|m|mins?|minutes?|h|hrs?|hours?)?$/
  );
  if (relTimeMatches?.groups) {
    const { sign, value, units } = relTimeMatches?.groups;
    const durationUnit =
      units === undefined
        ? "minutes"
        : units[0] === "s"
        ? "seconds"
        : units[0] === "m"
        ? "minutes"
        : units[0] === "h"
        ? "hours"
        : "minutes";

    const durationObject: { [index: string]: number } = {};
    durationObject[durationUnit] = Number(value);

    if (sign === "+") {
      return referenceTime.plus(Duration.fromObject(durationObject));
    } else {
      return referenceTime.minus(Duration.fromObject(durationObject));
    }
  }

  // DateTime can parse some extra ISO type strings
  const dateTimeParsed = DateTime.fromISO(timeStr);
  if (dateTimeParsed.isValid) {
    return dateTimeParsed;
  }

  // As a last resort, use chrono to parse the time
  const chronoParsed = chrono.parseDate(timeStr, referenceTime.toJSDate());
  if (chronoParsed) {
    return DateTime.fromISO(chronoParsed.toISOString());
  }

  throw new BadTimeArgError("time not parsable");
};

const pluralize = require("pluralize");
const { DateTime, Duration } = require("luxon");

type isoDatetime = string;
type isoDate = string;
type DateTime = any;

const currentTime = DateTime.local() as DateTime;

type ProcessTimeArgsType = {
  date?: string;
  time?: string;
  yesterday?: number;
  quick?: number;
  fullDay?: boolean;
  referenceTime?: DateTime;
};
const processTimeArgs = function ({
  date,
  time,
  yesterday,
  quick,
  fullDay,
  referenceTime = currentTime,
}: ProcessTimeArgsType): isoDatetime | isoDate {
  // This happens often because data is collected as it happens. It needs to be fast so checked first
  if (!date && !time && !yesterday && !quick) {
    return referenceTime.toUTC().toString();
  }
  // likewise with yesterday and day
  const dateStr = date ?? (yesterday ? (-1 * yesterday).toString() : undefined);

  if (time) {
    referenceTime = parseTimeStr({ timeStr: time, referenceTime });
  }

  if (quick) {
    referenceTime = referenceTime.minus({ minutes: 5 * quick });
  }

  return referenceTime.toUTC().toString();
};


type ParseTimeStrType = {
  timeStr: string;
  referenceTime: DateTime;
};
const parseTimeStr = function ({
  timeStr,
  referenceTime,
}: ParseTimeStrType): DateTime {
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
      meridian?.toLowerCase() === "pm" ? parseInt(hour) + 12 : parseInt(hour);

    // if less than all three milliseconds are given, fill the remaining zeroes
    const millisecond = (milli + "000").substring(0, 3);

    return referenceTime.set({
      hour: correctedHour,
      minute,
      second,
      millisecond,
    });
  }

  // Also supports relative time strings, e.g., -5min
  const relTimeMatches = timeStr.match(
    /^(?<sign>\+|-)(?<value>\d+(\.\d)?) ?(?<units>s|secs?|seconds?|m|mins?|minutes?|h|hrs?|hours?)?$/
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
    
    console.log(durationObject)
    if (sign === "+") {
      return referenceTime.plus(Duration.fromObject(durationObject));
    } else {
      return referenceTime.minus(Duration.fromObject(durationObject));
    }
  }

  // DateTime can parse some extra ISO type strings
  const dateTimeParsed = DateTime.fromISO(timeStr);
  if (dateTimeParsed.invalid === null) {
    return dateTimeParsed;
  }

  // As a last resort, use chrono to parse the time
  const chrono = require("chrono-node");
  const chronoParsed = chrono.parseDate(timeStr, referenceTime.toJSDate());
  if (chronoParsed) {
    return DateTime.fromISO(chronoParsed.toISOString());
  }

  class BadTimeArgError extends Error {
    constructor(message: string) {
      super(message);
      this.name = this.constructor.name;
    }
  }
  throw BadTimeArgError;
};

module.exports = { processTimeArgs, currentTime };

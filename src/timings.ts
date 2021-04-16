import { DateTime, Duration, Settings as DateTimeSettings } from "luxon";
import * as chrono from "chrono-node";

type isoDatetime = string;
type isoDate = string;

type TimingData = {
  occurTime?: isoDatetime | isoDate;
  createTime: isoDatetime;
  modifyTime: isoDatetime;
  utcOffset: number;
};

type ProcessTimeArgsType = {
  date?: string;
  time?: string;
  yesterday?: number;
  quick?: number;
  fullDay?: boolean;
  noTimestamp?: boolean;
  referenceTime?: DateTime;
  timezone?: string;
};
export const processTimeArgs = function ({
  date,
  time,
  yesterday,
  quick,
  fullDay,
  noTimestamp,
  referenceTime,
  timezone,
}: ProcessTimeArgsType): TimingData {
  if (timezone) {
    if (isNaN(Number(timezone))) {
      // timezone is a named zone
      DateTimeSettings.defaultZoneName = timezone;
    } else {
      // timezone is a utc offset "+6"
      DateTimeSettings.defaultZoneName = `UTC${timezone}`;
    }
  }

  const now = DateTime.local() as DateTime;
  referenceTime = referenceTime ?? now;

  const baseMetadata = {
    createTime: now.toUTC().toString(),
    modifyTime: now.toUTC().toString(),
    utcOffset: referenceTime.offset / 60,
  };

  if (noTimestamp) {
    return baseMetadata;
  }

  if (time) {
    referenceTime = parseTimeStr({ timeStr: time, referenceTime });
  }

  if (quick) {
    referenceTime = referenceTime.minus({ minutes: 5 * quick });
  }

  if (date) {
    referenceTime = parseDateStr({ dateStr: date, referenceTime });
  }

  if (yesterday) {
    referenceTime = referenceTime.minus({ days: yesterday });
  }

  // if only date information is given (or marked fullDay), only record the date
  const occurTime =
    fullDay || ((date || yesterday) && !time && !quick)
      ? (referenceTime.toISODate() as isoDate)
      : (referenceTime.toUTC().toString() as isoDatetime);

  return {
    ...baseMetadata,
    occurTime,
    utcOffset: referenceTime.offset / 60, // utc offset needs to be recalculated because DST could be different for the specified time, for example.
  };
};

type ParseTimeStrType = {
  timeStr: string;
  referenceTime: DateTime;
};
export const parseTimeStr = function ({
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

type ParseDateStrType = {
  dateStr: string;
  referenceTime: DateTime;
};
const parseDateStr = function ({
  dateStr,
  referenceTime,
}: ParseDateStrType): DateTime {
  // Relative dates, e.g. can use -1 to mean yesterday or +1 to mean tomorrow
  const relDateMatches = dateStr.match(/^([+-])\d+$/);
  if (relDateMatches) {
    return referenceTime.plus(
      Duration.fromObject({ days: parseInt(relDateMatches[0], 10) })
    );
  }

  // DateTime can parse some extra ISO type strings
  const dateTimeParsed = DateTime.fromISO(dateStr);
  if (dateTimeParsed.isValid) {
    // Only want to change the date on the relative time
    const { year, month, day } = dateTimeParsed.toObject();
    return referenceTime.set({ year, month, day });
  }

  // Finally, use chrono to parse the time if all else fails
  // The keepLocalTime is to avoid timezone shenanigans
  const chronoParsed = chrono.parseDate(
    dateStr,
    referenceTime.toUTC(0, { keepLocalTime: true }).toJSDate()
  );
  if (chronoParsed) {
    const { year, month, day } = DateTime.fromISO(
      chronoParsed.toISOString()
    ).toObject();
    return referenceTime.set({ year, month, day });
  }

  throw new BadDateArgError("date not parsable");
};

export class BadTimeArgError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

export class BadDateArgError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

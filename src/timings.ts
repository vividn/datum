const pluralize = require("pluralize");
const { DateTime } = require("luxon");

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

  // quick should theoretically never coincide with time from yargs
  const timeStr = time ?? (quick ? (-5 * quick).toString() : undefined);
  // likewise with yesterday and day
  const dateStr = date ?? (yesterday ? (-1 * yesterday).toString() : undefined);

  if (timeStr) {
    referenceTime = parseTimeStr({timeStr, referenceTime})
  }

  return referenceTime.toUTC().toString();
};


type ParseTimeStrType = {
  timeStr: string;
  referenceTime: DateTime
}
const parseTimeStr = function({
  timeStr,
  referenceTime
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

  // DateTime can parse some extra ISO type strings
  const dateTimeParsed = DateTime.fromISO(timeStr)
  if (dateTimeParsed.invalid === null) {
    return dateTimeParsed
  }

  // As a last resort, use chrono to parse the time
  const chrono = require("chrono-node");
  return DateTime.fromISO(chrono.parseDate(timeStr, referenceTime.toJSDate()).toISOString())
}

module.exports = { processTimeArgs, currentTime }
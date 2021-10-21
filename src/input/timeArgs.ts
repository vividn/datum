import yargs, { Argv } from "yargs";
import { DateTime } from "luxon";
import { isoDate, isoDatetime, now } from "../time/timeUtils";
import { setTimezone } from "../time/setTimezone";
import { parseTimeStr } from "../time/parseTimeStr";
import { parseDateStr } from "../time/parseDateStr";

export type TimeArgs = {
  date?: string;
  yesterday?: number;
  time?: string;
  quick?: number;
  timezone?: string;
  fullDay?: boolean;
  noTimestamp?: boolean;
};

export function timeYargs(otherYargs?: Argv): Argv {
  const yarg = otherYargs ?? yargs;
  return yarg
    .group(
      [
        "date",
        "yesterday",
        "time",
        "quick",
        "timezone",
        "full-day",
        "no-timestamp",
      ],
      "Timing"
    )
    .options({
      date: {
        describe:
          "date of the timestamp, use `+n` or `-n` for a date relative to today. If no time is specified with -t, -T is assumed.",
        alias: "d",
        nargs: 1,
        type: "string",
      },
      yesterday: {
        describe:
          "use yesterday's date. Equivalent to `-d yesterday`. Use multiple times to go back more days",
        alias: "y",
        type: "count",
      },
      time: {
        describe:
          "specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now",
        alias: "t",
        nargs: 1,
        type: "string",
      },
      quick: {
        describe:
          "quick options for time, use multiple times. -q = 5 min ago, -qq = 10 min ago, etc.",
        alias: "q",
        type: "count",
      },
      timezone: {
        describe:
          "Set the timezone to use instead of local time. Accepts both timezone names (America/Chicago) and utc offsets '-7'",
        alias: "z",
        type: "string",
      },
      "full-day": {
        describe:
          "make an entry for the full day, without a specific timestamp, occurs also when -d is used without -t",
        alias: "D",
        type: "boolean",
      },
      "no-timestamp": {
        describe: "omit the occurTime from the data",
        alias: "T",
        type: "boolean",
      },
    });
}

export type ReferencedTimeArgs = TimeArgs & {
  referenceTime?: DateTime;
};
export type TimeStrWithOffset = {
  timeStr?: isoDatetime | isoDate;
  utcOffset: number;
};
export const handleTimeArgs = function ({
  date,
  time,
  yesterday,
  quick,
  fullDay,
  timezone,
  noTimestamp,
  referenceTime,
}: ReferencedTimeArgs): TimeStrWithOffset {
  const tzOffset = setTimezone(timezone);
  if (noTimestamp) {
    return {
      timeStr: undefined,
      utcOffset: tzOffset,
    };
  }

  referenceTime = referenceTime ?? now();

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
  const timeStr =
    fullDay || ((date || yesterday) && !time && !quick)
      ? (referenceTime.toISODate() as isoDate)
      : (referenceTime.toUTC().toString() as isoDatetime);

  return {
    timeStr,
    utcOffset: referenceTime.offset / 60, // utc offset needs to be recalculated because DST could be different for the specified time, for example.
  };
};
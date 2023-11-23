import yargs, { Argv } from "yargs";
import { DateTime } from "luxon";
import { DatumTime, defaultZone, isoDate, isoDatetime, now, toDatumTime, utcOffset } from "../time/timeUtils";
import { getTimezone } from "../time/getTimezone";
import { parseTimeStr } from "../time/parseTimeStr";
import { parseDateStr } from "../time/parseDateStr";
import { DataArgs, parseBaseData } from "./dataArgs";
import { DatumData } from "../documentControl/DatumDocument";
import { getOccurTime } from "../time/getOccurTime";

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
export type TimeFromArgs = {
  time?: DatumTime;
  luxon?: DateTime;
  unmodified: boolean;
  onlyDate: boolean;
};
export function handleTimeArgs({
  date,
  time,
  yesterday,
  quick,
  fullDay,
  timezone,
  noTimestamp,
  referenceTime,
}: ReferencedTimeArgs): TimeFromArgs {
  const tz = getTimezone(timezone);
  referenceTime = referenceTime ?? now(tz);
  let unmodified = true;
  if (noTimestamp) {
    return {
      unmodified: false,
      onlyDate: false,
    };
  }

  if (time) {
    referenceTime = parseTimeStr({ timeStr: time, referenceTime });
    unmodified = false;
  }

  if (quick) {
    referenceTime = referenceTime.minus({ minutes: 5 * quick });
    unmodified = false;
  }

  if (date) {
    referenceTime = parseDateStr({ dateStr: date, referenceTime });
    unmodified = false;
  }

  if (yesterday) {
    referenceTime = referenceTime.minus({ days: yesterday });
    unmodified = false;
  }

  // if only date information is given (or marked fullDay), only record the date
  const onlyDate = !!(fullDay || ((date || yesterday) && !time && !quick));
  const timeStr = onlyDate
    ? (referenceTime.toISODate() as isoDate)
    : (referenceTime.toUTC().toString() as isoDatetime);

  return {
    time: toDatumTime(referenceTime),
    luxon: referenceTime,
    unmodified,
    onlyDate,
  };
}

// TODO: Consider moving this function into the test file, and creating a new function that takes a given occurTime after parsing the data as the reference time
export function occurredBaseArgs(
  args: ReferencedTimeArgs & Pick<DataArgs, "baseData">
): DatumData {
  const parsedData = parseBaseData(args.baseData);
  const baseOccurTime = getOccurTime(parsedData);
  const referenceTime = args.referenceTime ?? baseOccurTime;
  const { timeStr: occurTime, utcOffset: occurUtcOffset } = handleTimeArgs({
    ...args,
    referenceTime,
  });
  return {
    ...parsedData,
    occurTime,
    occurUtcOffset,
  };
}

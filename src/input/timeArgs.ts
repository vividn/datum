import { DateTime } from "luxon";
import { DatumTime, now, toDatumTime } from "../time/timeUtils";
import { getTimezone } from "../time/getTimezone";
import { parseTimeStr } from "../time/parseTimeStr";
import { parseDateStr } from "../time/parseDateStr";
import { DataArgs, parseBaseData } from "./dataArgs";
import { DatumData } from "../documentControl/DatumDocument";
import { getOccurTime } from "../time/getOccurTime";
import { ArgumentParser, BooleanOptionalAction } from "argparse";

export type TimeArgs = {
  date?: string;
  yesterday?: number;
  time?: string;
  quick?: number;
  timezone?: string;
  fullDay?: boolean;
  omitTimestamp?: boolean;
};

export const timeArgs = new ArgumentParser({
  add_help: false,
});
const timeGroup = timeArgs.add_argument_group({
  title: "Timing",
  description: "Options for specifying the time of the data",
});
timeGroup.add_argument("-d", "--date", {
  help: "date of the timestamp, use `+n` or `-n` for a date relative to today. If no time is specified with -t, -T is assumed.",
  type: "str",
});
timeGroup.add_argument("-y", "--yesterday", {
  help: "use yesterday's date. Equivalent to `-d yesterday`. Use multiple times to go back more days",
  action: "count",
});
timeGroup.add_argument("-t", "--time", {
  help: "specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now",
  type: "str",
});
timeGroup.add_argument("-q", "--quick", {
  help: "quick options for time, use multiple times. -q = 5 min ago, -qq = 10 min ago, etc.",
  action: "count",
});
timeGroup.add_argument("-z", "--timezone", {
  help: "Set the timezone to use instead of local time. Accepts both timezone names (America/Chicago) and utc offsets '-7'",
  type: "str",
});
timeGroup.add_argument("-D", "--full-day", {
  help: "make an entry for the full day, without a specific timestamp, occurs also when -d is used without -t",
  action: BooleanOptionalAction,
  dest: "fullDay",
});
timeGroup.add_argument("-T", "--omit-timestamp", {
  help: "omit the occurTime from the data",
  action: BooleanOptionalAction,
  dest: "omitTimestamp",
});

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
  omitTimestamp,
  referenceTime,
}: ReferencedTimeArgs): TimeFromArgs {
  const tz = getTimezone(timezone);
  referenceTime = referenceTime ?? now(tz);
  let unmodified = true;
  if (omitTimestamp) {
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

  return {
    time: toDatumTime(referenceTime, onlyDate),
    luxon: !onlyDate ? referenceTime : undefined,
    unmodified,
    onlyDate,
  };
}

// TODO: Consider moving this function into the test file, and creating a new function that takes a given occurTime after parsing the data as the reference time
export function occurredBaseArgs(
  args: ReferencedTimeArgs & Pick<DataArgs, "baseData">,
): DatumData {
  const parsedData = parseBaseData(args.baseData);
  const baseOccurTime = getOccurTime(parsedData);
  const referenceTime = args.referenceTime ?? baseOccurTime;
  const { time: occurTime } = handleTimeArgs({
    ...args,
    referenceTime,
  });
  return {
    ...parsedData,
    occurTime,
  };
}

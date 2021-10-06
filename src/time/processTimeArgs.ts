import { isoDate, isoDatetime, now } from "./timeUtils";
import { TimingInputArgs } from "../input/timingArgs";
import { DateTime } from "luxon";
import { parseTimeStr } from "./parseTimeStr";
import { parseDateStr } from "./parseDateStr";
import { setTimezone } from "./setTimezone";

export type ProcessTimeArgsType = TimingInputArgs & {
  referenceTime?: DateTime;
};
export type TimingData = {
  timeStr?: isoDatetime | isoDate;
  utcOffset: number;
};
export const processTimeArgs = function ({
  date,
  time,
  yesterday,
  quick,
  fullDay,
  referenceTime,
  timezone,
  noTimestamp,
}: ProcessTimeArgsType): TimingData {
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
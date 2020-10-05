const pluralize = require("pluralize");
const chrono = require("chrono-node");
var { DateTime } = require("luxon");

type isoDatetime = string;
type isoDate = string;

const currentTime = DateTime.local();

type ProcessTimeArgsType = {
  date?: string,
  time?: string,
  yesterday?: number,
  fullDay?: boolean,
  quick?: number
}
const processTimeArgs = function ({
  date,
  time,
  yesterday,
  fullDay,
  quick,
}: ProcessTimeArgsType): isoDatetime | isoDate {
  
  // This happens often because data is collected as it happens. It needs to be fast so checked first
  if (!date && !time && !yesterday && !quick) {
    return currentTime.toUTC().toString();
  }

  const dateStr =
    date ??
    (yesterday ? relTimeStr(-1 * yesterday, "days") : undefined) ??
    (fullDay ? "today" : undefined);
  const timeStr =
    time ?? (quick ? relTimeStr(-5 * quick, "minutes") : undefined);

  return combineDateTime(dateStr, timeStr);
};

const combineDateTime = function (
  dateStr?: string,
  timeStr?: string
): isoDatetime | isoDate {
  if (!dateStr && !timeStr) {
    return currentTime.toISOString() as isoDatetime;
  } else if (dateStr && !timeStr) {
    return chrono.parseDate(dateStr).toISOString().split("T")[0] as isoDate;
  } else if (!dateStr && timeStr) {
    return chrono.parseDate(timeStr).toISOString() as isoDatetime;
  } else {
    const fullTimeString = `${dateStr} at ${timeStr}`;
    return chrono.parseDate(fullTimeString).toISOString() as isoDatetime;
  }
};

const relTimeStr = function (n: number, unit = "days"): string {
  const nAbs = Math.abs(n);
  const unitStr = pluralize(unit, nAbs);
  return n < 0 ? `${nAbs} ${unitStr} ago` : `${n} ${unitStr} from now`;
};

module.exports = { processTimeArgs, currentTime, combineDateTime, relTimeStr };

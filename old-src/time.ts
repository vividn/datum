import pluralize from 'pluralize';
import * as chrono from 'chrono-node';
import { strIndObj } from './utils';

// Take a timestamp as soon as possible for accuracy
export const currentTime = new Date();

export const relTimeStr = function (n: number, unit = 'days'): string {
  const unitStr = pluralize(unit, Math.abs(n));
  return n < 0 ? `${Math.abs(n)} ${unitStr} ago` : `${n} ${unitStr} from now`;
};

type ProcessTimeArgsType = {
  date?: string;
  time?: string;
  yesterday?: number;
  fullDay?: boolean;
  quick?: number;
};
export function processTimeArgs({
  date,
  time,
  yesterday,
  fullDay,
  quick,
}: ProcessTimeArgsType): strIndObj {
  if (
    date === undefined &&
    time === undefined &&
    yesterday === undefined &&
    quick === undefined
  ) {
    // This happens often when arguments are given, needs to be fast so checked first
    return {
      occurTime: currentTime.toISOString(),
      creationTime: currentTime.toISOString(),
    };
  }
  const dateStr =
    date ??
    (yesterday ? relTimeStr(-1 * yesterday, 'days') : undefined) ??
    (fullDay ? 'today' : undefined);
  const timeStr =
    time ?? (quick ? relTimeStr(-5 * quick, 'minutes') : undefined);

  return {
    occurTime: combineDateTime(dateStr, timeStr),
    creationTime: currentTime.toISOString(),
  };
}

export const combineDateTime = function (
  dateStr?: string,
  timeStr?: string
): string {
  if (!dateStr && !timeStr) {
    return currentTime.toISOString();
  } else if (dateStr && !timeStr) {
    return chrono.parseDate(dateStr).toISOString().split('T')[0];
  } else if (!dateStr && timeStr) {
    return chrono.parseDate(timeStr).toISOString();
  } else {
    const fullTimeString = `${dateStr} at ${timeStr}`;
    return chrono.parseDate(fullTimeString).toISOString();
  }
};

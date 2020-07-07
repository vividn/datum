const pluralize = require('pluralize');
const chrono = require('chrono-node');

exports.relTimeStr = function(n: number, unit = 'days'): string {
  const unitStr = pluralize(unit, Math.abs(n));
  return n < 0 ? `${Math.abs(n)} ${unitStr} ago` : `${n} ${unitStr} from now`;
};

exports.combineDateTime = function(
  dateStr?: string,
  timeStr?: string,
  currentTime?: Date
): string {
  if (!dateStr && !timeStr) {
    const now = currentTime ?? new Date();
    return now.toISOString();
  } else if (dateStr && !timeStr) {
    return chrono
      .parseDate(dateStr)
      .toISOString()
      .split('T')[0];
  } else if (!dateStr && timeStr) {
    return chrono.parseDate(timeStr).toISOString();
  } else {
    const fullTimeString = `${dateStr} at ${timeStr}`;
    return chrono.parseDate(fullTimeString).toISOString();
  }
};

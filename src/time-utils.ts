const pluralize = require('pluralize');

exports.relTimeStr = function(n: number, unit = 'days'): string {
  const unitStr = pluralize(unit, Math.abs(n));
  return n < 0 ? `${Math.abs(n)} ${unitStr} ago` : `${n} ${unitStr} from now`;
};

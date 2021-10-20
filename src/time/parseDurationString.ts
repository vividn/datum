import { Duration } from "luxon";
import parse from "parse-duration";
import { BadDurationArgError } from "../errors";

type ParseDurationStrType = {
  durationStr: string;
};

export const parseDurationStr = function ({
  durationStr,
}: ParseDurationStrType): Duration {
  let isNegative;
  switch (durationStr.charAt(0)) {
    case "-":
      isNegative = true;
      durationStr = durationStr.slice(1);
      break;

    case "+":
      isNegative = false;
      durationStr = durationStr.slice(1);
      break;

    default:
      isNegative = false;
      break;
  }

  const durFromIso = Duration.fromISO(durationStr);
  if (durFromIso.isValid) {
    return isNegative ? durFromIso.negate() : durFromIso;
  }

  // 10             -> 10min
  // 10:11          -> 10min 11sec
  // 10:11:12       -> 10hrs 11min 12sec
  // 10:11:12:13    -> 10days 11hours 12min 13sec
  const colonSplits = durationStr.split(':').map((element) => Number(element));
  if (colonSplits.length > 1 && colonSplits.every((value) => !Number.isNaN(value))) {
    const [seconds, minutes, hours, days, years, extra] = colonSplits.reverse();

    if (extra !== undefined) {
      throw new BadDurationArgError("could not parse duration");
    }

    const duration = Duration.fromObject({seconds, minutes, hours, days, years});
    return isNegative ? duration.negate() : duration;
  }

  // no units should mean minutes
  parse[""] = parse["min"];

  const ms = parse(durationStr);

  if (ms === null) {
    throw new BadDurationArgError("could not parse duration");
  }

  const duration = Duration.fromObject({ milliseconds: ms }).shiftTo(
    "days",
    "hours",
    "minutes",
    "seconds",
    "milliseconds"
  );
  return isNegative ? duration.negate() : duration;
};

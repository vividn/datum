import { DateTime, Duration } from "luxon";
import { now } from "./timeUtils";
import * as chrono from "chrono-node";
import { BadDateError } from "../errors";

export type ParseDateStrType = {
  dateStr: string;
  referenceTime?: DateTime;
};
export function parseDateStr({
  dateStr,
  referenceTime,
}: ParseDateStrType): DateTime {
  referenceTime = referenceTime ?? now();

  // Relative dates, e.g. can use -1 to mean yesterday or +1 to mean tomorrow
  const relDateMatches = dateStr.match(/^([+-])\d+$/);
  if (relDateMatches) {
    return referenceTime.plus(
      Duration.fromObject({ days: parseInt(relDateMatches[0], 10) }),
    );
  }

  // DateTime can parse some extra ISO type strings
  const dateTimeParsed = DateTime.fromISO(dateStr);
  if (dateTimeParsed.isValid) {
    // Only want to change the date on the relative time
    const { year, month, day } = dateTimeParsed.toObject();
    return referenceTime.set({ year, month, day });
  }

  // Finally, use chrono to parse the time if all else fails
  // The keepLocalTime is to avoid timezone shenanigans
  // i.e. tomorrow is always local tomorrow not utc tomorrow
  const chronoParsed = chrono.parseDate(
    dateStr,
    referenceTime.toUTC(0, { keepLocalTime: true }).toJSDate(),
  );
  if (chronoParsed) {
    const { year, month, day } = DateTime.fromJSDate(chronoParsed, {
      zone: "utc",
    }).toObject();
    return referenceTime.set({ year, month, day });
  }

  throw new BadDateError("date not parsable");
}

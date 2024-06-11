import chalk from "chalk";
import { DateTime, Interval } from "luxon";
import { DatumTime, datumTimeToLuxon } from "./timeUtils";

export function humanTime(time?: DatumTime | string): string | undefined {
  if (!time) {
    return undefined;
  }
  // TODO: remove this once all docs are updated to use DatumTime
  if (typeof time === "string") {
    time = { utc: time };
  }
  // if time is just a date, then return it
  if (!time.utc.includes("T")) {
    const future = time.utc > (DateTime.now().toISODate() ?? time.utc);
    return future ? chalk.underline(time.utc) : time.utc;
  }

  const dateTime = datumTimeToLuxon(time);
  if (dateTime === undefined || !dateTime.isValid) {
    return undefined;
  }

  let dateText: string;
  const date = dateTime.toISODate() as string;
  const today = DateTime.now().toISODate() as string;
  if (date === today) {
    dateText = "";
  } else if (date < today) {
    const days = Interval.fromDateTimes(
      DateTime.fromISO(date),
      DateTime.fromISO(today),
    ).toDuration("days").days;
    dateText = days > 3 ? date : "-" + days + "d";
  } else {
    const days = Interval.fromDateTimes(
      DateTime.fromISO(today),
      DateTime.fromISO(date),
    ).toDuration("days").days;
    dateText = days > 3 ? date : "+" + days + "d";
  }
  const offsetText = chalk.gray(dateTime.toFormat("Z"));
  const timeText = dateTime.toFormat("HH:mm:ss") + offsetText;
  const fullText = [dateText, timeText].filter(Boolean).join(" ");
  const future = dateTime > DateTime.now();
  return future ? chalk.underline(fullText) : fullText;
}

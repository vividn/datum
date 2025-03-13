import { DateTime } from "luxon";
import * as d3 from "d3";
import { allFieldsSvg } from "./allFieldsSvg";
import { domdoc } from "./domdoc";

export type SingleDayArgs = {
  db: PouchDB.Database;
  date: string;
  height: number;
  dataWidth: number;
  labelWidth: number;
};

export async function singleDay(args: SingleDayArgs) {
  const { db, date, dataWidth, height } = args;

  // For now just use local time
  // TODO: get timezone stats and handle timezone changes

  const day: DateTime<true> = DateTime.fromISO(date) as DateTime<true>;
  const startUtc = day.startOf("day").toUTC().toISO();
  const endUtc = day.endOf("day").toUTC().toISO();

  const document = domdoc();
  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("class", `day ${day.toISODate()}`);

  try {
    const dayData = await allFieldsSvg({
      db,
      startUtc,
      endUtc,
      width: dataWidth,
      height,
    });
    svg.append(() => dayData);
  } catch (error) {
    console.error(`Error loading data for ${date}:`, error);
    svg.append("use").attr("href", "#warning-icon").attr("height", height);
  }

  return svg.node();
}

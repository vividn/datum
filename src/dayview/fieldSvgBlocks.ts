import * as d3 from "d3";
import { checkState, StateChangeRow } from "../state/checkState";
import { DatumState } from "../state/normalizeState";
import { isoDatetime } from "../time/timeUtils";
import { HIGH_STRING } from "../utils/startsWith";
import { stateChangeView } from "../views/datumViews";
import { PointDataRow, pointDataView } from "../views/datumViews/pointDataView";
import { domdoc } from "./domdoc";
import { simplifyState } from "../state/simplifyState";
import { getStateColor } from "../field/fieldColor";

export type FieldSvgBlocksType = {
  db: PouchDB.Database;
  field: string;
  startUtc: isoDatetime;
  endUtc: isoDatetime;
  width: number;
  height: number;
};
type DBlock = {
  time: Date;
  state: DatumState;
};
export async function fieldSvgBlocks(args: FieldSvgBlocksType) {
  const { db, field, startUtc, endUtc, width, height } = args;

  const [blockRows, pointRows] = await Promise.all([
    (
      await db.query(stateChangeView.name, {
        reduce: false,
        startkey: [field, startUtc ?? ""],
        endkey: [field, endUtc ?? HIGH_STRING],
      })
    ).rows as StateChangeRow[],
    (
      await db.query(pointDataView.name, {
        reduce: false,
        startkey: [field, startUtc ?? ""],
        endkey: [field, endUtc ?? HIGH_STRING],
      })
    ).rows as PointDataRow[],
  ]);
  const initialState: DatumState =
    (
      await db.query(stateChangeView.name, {
        reduce: false,
        startkey: [field, startUtc?.slice(0, -1) ?? ""],
        endkey: [field, ""],
        descending: true,
        limit: 1,
      })
    ).rows[0]?.value[1] ?? null;
  const blocks: DBlock[] = blockRows.map((row) => {
    return {
      time: new Date(row.key[1]),
      state: row.value[1],
    };
  });
  const points = pointRows.map((row) => {
    return {
      time: new Date(row.key[1]),
      state: row.value,
    };
  });
  blocks.unshift({
    time: new Date(startUtc),
    state: initialState,
  });

  const now = new Date();
  const endTime = new Date(endUtc);
  const lastBlockTime = now < endTime ? now : endTime;
  blocks.push({
    time: lastBlockTime,
    state: blocks.at(-1)!.state,
  });

  type DPair = [DBlock, DBlock];
  const dataPairs: DPair[] = d3.pairs(blocks);

  const document = domdoc();
  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("class", `${field}`)
    .attr("viewBox", [0, 0, width, height].join(" "))
    .attr("preserveAspectRatio", "none");

  const timeScale = d3
    .scaleTime()
    .domain([new Date(startUtc), new Date(endUtc)])
    .range([0, width]);

  dataPairs.forEach(([curr, next]) => {
    const simpleState = simplifyState(curr.state);
    const state = Array.isArray(simpleState) // TODO: make a pattern representing array states
      ? JSON.stringify(simpleState)
      : simpleState;
    if (state === null || state === false) {
      return;
    }
    const color = getStateColor({ state, field });
    svg
      .append("rect")
      .attr("class", `${field} ${state} block`)
      .attr("x", timeScale(curr.time))
      .attr("y", 0)
      .attr("width", timeScale(next.time) - timeScale(curr.time))
      .attr("height", "100%")
      .attr("fill", color);
  });

  const five_minutes = timeScale(new Date(startUtc).valueOf() + 5 * 60 * 1000);
  const circle_r = Math.min(five_minutes, height / 2, width / 4, 10);
  points.forEach((point) => {
    const simpleState = simplifyState(point.state);
    const state = Array.isArray(simpleState) // TODO: make a pattern representing array states
      ? JSON.stringify(simpleState)
      : simpleState;
    const color = getStateColor({ state, field });

    svg
      .append("circle")
      .attr("class", `${field} ${state} point`)
      .attr("cx", timeScale(point.time))
      .attr("cy", height / 2)
      .attr("r", circle_r)
      .attr("fill", color);
  });

  const fieldErrors = await checkState({
    db,
    field,
    failOnError: false,
    startTime: startUtc,
    endTime: endUtc,
  });

  const warning_r = circle_r * 1.5;
  for (const error of fieldErrors.errors) {
    console.error(error.message);
    svg
      .append("use")
      .attr("xlink:href", "#warning-icon")
      .attr("x", timeScale(new Date(error.occurTime)) - warning_r)
      .attr("y", height / 2 - warning_r)
      .attr("width", warning_r * 2)
      .attr("height", warning_r * 2)
      .attr("class", `${field} error`)
      .attr("field", field);
  }

  // don't append an svg block for empty data
  if (svg.selectChildren().size() === 0) {
    svg.remove();
    return null;
  }

  return svg.node();
}

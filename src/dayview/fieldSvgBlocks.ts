import * as d3 from "d3";
import { JSDOM } from "jsdom";
import { StateChangeRow } from "../state/checkState";
import { DatumState } from "../state/normalizeState";
import { isoDatetime } from "../time/timeUtils";
import { HIGH_STRING } from "../utils/startsWith";
import { stateChangeView } from "../views/datumViews";
import { md5Color } from "../utils/md5Color";
import { PointDataRow, pointDataView } from "../views/datumViews/pointDataView";

export type FieldSvgBlocksType = {
  db: PouchDB.Database;
  field: string;
  startUtc: isoDatetime;
  endUtc: isoDatetime;
};
type DBlock = {
  time: Date;
  state: DatumState;
};
export async function fieldSvgBlocks(args: FieldSvgBlocksType) {
  const { db, field, startUtc, endUtc } = args;

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

  const document = new JSDOM().window.document;
  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("class", `${field}`)
    .attr("viewBox", "0 0 1 1")
    .attr("preserveAspectRatio", "none");

  const timeScale = d3
    .scaleTime()
    .domain([new Date(startUtc), new Date(endUtc)])
    .range([0, 1]);

  dataPairs.forEach(([curr, next]) => {
    const state = curr.state;
    if (state === null || state === false) {
      return;
    }
    const color =
      state === true
        ? md5Color(field)
        : typeof state === "string"
          ? md5Color(state)
          : md5Color(String(state));
    svg
      .append("rect")
      .attr("class", `${field} ${state} block`)
      .attr("x", timeScale(curr.time))
      .attr("y", 0)
      .attr("width", `${100 * (timeScale(next.time) - timeScale(curr.time))}%`)
      .attr("height", "100%")
      .attr("fill", color);
  });
  points.forEach((point) => {
    const state = point.state;
    const color =
      state === null || state === false
        ? "black"
        : state === true
          ? md5Color(field)
          : md5Color(String(state));

    svg
      .append("circle")
      .attr("class", `${field} ${state} point`)
      .attr("cx", timeScale(point.time))
      .attr("cy", 0.5)
      .attr("r", 0.1)
      .attr("fill", color);
  });

  // don't append an svg block for empty data
  if (svg.selectChildren().size() === 0) {
    svg.remove();
    return null;
  }

  const node = svg.node();
  if (node === null) {
    throw new Error("field data incorrectly created");
  }
  return node;
}

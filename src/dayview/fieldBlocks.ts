import d3 from "d3";
import { StateChangeRow } from "../state/checkState";
import { DatumState } from "../state/normalizeState";
import { isoDatetime } from "../time/timeUtils";
import { HIGH_STRING } from "../utils/startsWith";
import { stateChangeView } from "../views/datumViews";
import { md5Color } from "../utils/md5Color";

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
  const rows = (
    await db.query(stateChangeView.name, {
      reduce: false,
      startkey: [field, startUtc ?? ""],
      endkey: [field, endUtc ?? HIGH_STRING],
    })
  ).rows as StateChangeRow[];
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
  const blocks: DBlock[] = rows.map((row) => {
    return {
      time: new Date(row.key[1]),
      state: row.value[1],
    };
  });
  blocks.unshift({
    time: new Date(startUtc),
    state: initialState,
  });
  blocks.push({
    time: new Date(endUtc),
    state: blocks.at(-1)!.state,
  });

  type DPair = [DBlock, DBlock];
  const dataPairs: DPair[] = d3.pairs(blocks);

  const timeScale = d3
    .scaleTime()
    .domain([new Date(startUtc), new Date(endUtc)])
    .range([0, 1]);

  const group = d3
    .create("g")
    .data(dataPairs)
    .enter()
    .append("rect")
    .attr("x", (d: DPair) => timeScale(d[0].time))
    .attr("y", 0)
    .attr("width", (d: DPair) => timeScale(d[1].time) - timeScale(d[0].time))
    .attr("height", 1)
    .attr("fill", (d) => {
      const state = d[0].state;
      if (state === null) {
        return "white";
      }
      if (state === true) {
        return md5Color(field);
      }
      if (state === false) {
        return "black";
      }
      if (typeof state === "string") {
        return md5Color(state);
      } else {
        return md5Color(String(state));
      }
    });

  return group;
}

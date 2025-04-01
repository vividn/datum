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

  // Add SVG definitions for patterns
  const defs = svg.append("defs");

  dataPairs.forEach(([curr, next]) => {
    const simpleState = simplifyState(curr.state);
    const state = Array.isArray(simpleState) ? simpleState : simpleState;
    if (state === null || state === false) {
      return;
    }

    // Format times in a more human-readable way
    const startTime = new Date(curr.time);
    const endTime = new Date(next.time);
    const formatTime = (date: Date) => {
      const hours = date.getHours().toString().padStart(2, "0");
      const minutes = date.getMinutes().toString().padStart(2, "0");
      return `${hours}:${minutes}`;
    };

    // Calculate duration
    const durationMs = endTime.getTime() - startTime.getTime();
    const durationMins = Math.round(durationMs / (60 * 1000));
    const durationHours = Math.floor(durationMins / 60);
    const remainingMins = durationMins % 60;
    const durationText =
      durationHours > 0
        ? `${durationHours}h ${remainingMins}m`
        : `${durationMins}m`;

    if (Array.isArray(state)) {
      // Calculate dimensions
      const blockWidth = timeScale(next.time) - timeScale(curr.time);
      const x = timeScale(curr.time);

      // Calculate the width of a 5-minute period on the time scale
      const fiveMinWidth =
        timeScale(new Date(curr.time.getTime() + 5 * 60 * 1000)) -
        timeScale(curr.time);

      // Create a unique pattern ID for this specific state combination
      const patternId = `stripe-pattern-${state.join("-")}-${x}`;

      // Create a pattern with diagonal stripes for each state
      const pattern = defs
        .append("pattern")
        .attr("id", patternId)
        .attr("patternUnits", "userSpaceOnUse")
        .attr("width", fiveMinWidth * state.length)
        .attr("height", height)
        .attr("patternTransform", "rotate(45)");

      // Each state gets a stripe of 5-minute width
      const stripeWidth = fiveMinWidth;

      // Add colored stripes to the pattern
      state.forEach((subState, index) => {
        const color = getStateColor({ state: subState, field });
        pattern
          .append("rect")
          .attr("x", index * stripeWidth)
          .attr("y", -height)
          .attr("width", stripeWidth)
          .attr("height", height * 3)
          .attr("fill", color);
      });

      // Format states for display
      const stateDisplay = state.join(", ");

      // Add the rectangle with the pattern fill and mouseover data
      svg
        .append("rect")
        .attr("class", `${field} multi-state block`)
        .attr("x", x)
        .attr("y", 0)
        .attr("width", blockWidth)
        .attr("height", height)
        .attr("fill", `url(#${patternId})`)
        .attr("data-field", field)
        .attr("data-state", JSON.stringify(state))
        .attr("data-time", curr.time.toISOString())
        .attr("data-end-time", next.time.toISOString())
        .append("title")
        .text(
          `Field: ${field}\nState: ${stateDisplay}\nTime: ${formatTime(startTime)} - ${formatTime(endTime)}\nDuration: ${durationText}`,
        );
    } else {
      const color = getStateColor({ state, field });
      svg
        .append("rect")
        .attr("class", `${field} ${state} block`)
        .attr("x", timeScale(curr.time))
        .attr("y", 0)
        .attr("width", timeScale(next.time) - timeScale(curr.time))
        .attr("height", "100%")
        .attr("fill", color)
        .attr("data-field", field)
        .attr("data-state", state)
        .attr("data-time", curr.time.toISOString())
        .attr("data-end-time", next.time.toISOString())
        .append("title")
        .text(
          `Field: ${field}\nState: ${state}\nTime: ${formatTime(startTime)} - ${formatTime(endTime)}\nDuration: ${durationText}`,
        );
    }
  });

  const five_minutes = timeScale(new Date(startUtc).valueOf() + 5 * 60 * 1000);
  const circle_r = Math.min(five_minutes, height / 2, width / 4, 10);
  points.forEach((point) => {
    const simpleState = simplifyState(point.state);
    const state = Array.isArray(simpleState)
      ? simpleState.join(", ")
      : simpleState;
    const color = getStateColor({ state, field });

    // Format time in a more human-readable way
    const pointTime = new Date(point.time);
    const formattedTime = `${pointTime.getHours().toString().padStart(2, "0")}:${pointTime.getMinutes().toString().padStart(2, "0")}`;

    svg
      .append("circle")
      .attr("class", `${field} ${state} point`)
      .attr("cx", timeScale(point.time))
      .attr("cy", height / 2)
      .attr("r", circle_r)
      .attr("fill", color)
      .attr("data-field", field)
      .attr("data-state", state)
      .attr("data-time", point.time.toISOString())
      .append("title")
      .text(`Field: ${field}\nState: ${state}\nTime: ${formattedTime}`);
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
    console.warn(error.message);
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

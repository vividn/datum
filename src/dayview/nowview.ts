import fs from "fs";
import * as d3 from "d3";
import { connectDb } from "../auth/connectDb";
import { NowviewCmdArgs } from "../commands/nowviewCmd";
import { domdoc } from "./domdoc";
import { allFieldsSvg } from "../dayview/allFieldsSvg";
import xmlFormatter from "xml-formatter";
import { now } from "../time/timeUtils";
import { DateTime } from "luxon";
import { xmlDeclaration } from "./xmlDeclaration";
import sharp from "sharp";
import { parseDurationStr } from "../time/parseDurationStr";

const DEFAULT_HEIGHT = 300;
const DEFAULT_TIME_AXIS_HEIGHT = 15;
const DEFAULT_NOW_WIDTH_MINUTES = 5; // Default width of current state in minutes

export async function nowview(args: NowviewCmdArgs): Promise<string> {
  const db = connectDb(args);

  // Parse history option if provided
  let historyMinutes = 0;
  if (args.history) {
    try {
      // Treat as a simple duration (e.g., "30m", "1h")
      const historyDuration = parseDurationStr({ durationStr: args.history });
      historyMinutes = Math.abs(historyDuration.as("minutes"));
    } catch {
      console.error(
        `Invalid history format: ${args.history}, defaulting to no history`,
      );
    }
  }

  // Parse now-width option if provided
  let nowWidthMinutes = DEFAULT_NOW_WIDTH_MINUTES;
  if (args.nowWidth) {
    try {
      // Treat as a simple duration (e.g., "5m", "10m")
      const nowWidthDuration = parseDurationStr({ durationStr: args.nowWidth });
      nowWidthMinutes = Math.abs(nowWidthDuration.as("minutes"));
    } catch {
      console.error(
        `Invalid now-width format: ${args.nowWidth}, using default of ${nowWidthMinutes} minutes`,
      );
    }
  }

  // Scale default width based on history size (100px for current state only, up to 400px for 15m+)
  let defaultWidth = 100; // Base width for current state only
  if (historyMinutes > 0) {
    // Scale from 200px at 5min to 400px at 15min
    if (historyMinutes <= 5) {
      defaultWidth = 200;
    } else if (historyMinutes <= 10) {
      defaultWidth = 300;
    } else {
      defaultWidth = 400;
    }
  }

  // Setup dimensions
  const width = args.width ?? defaultWidth;
  const height = args.height ?? DEFAULT_HEIGHT;
  const margin = { top: 30, right: 2, bottom: 15, left: 2 };
  const timeAxisHeight = args.timeAxisHeight ?? DEFAULT_TIME_AXIS_HEIGHT;

  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const dataHeight = plotHeight - timeAxisHeight;

  const endTime = now() as DateTime<true>;

  // Calculate start time based on history
  const startTime =
    historyMinutes > 0 ? endTime.minus({ minutes: historyMinutes }) : endTime;
  const startUtc = startTime.toUTC().toISO();
  const endUtc = endTime.toUTC().toISO();

  const document = domdoc("nowview");
  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("xmlns", "http://www.w3.org/2000/svg")
    .attr("xmlns:xlink", "http://www.w3.org/1999/xlink")
    .attr("width", width)
    .attr("height", height);

  // Add defs and styles like in dayview
  const defs = svg.append("defs");
  defs.append("style").text(`svg { overflow: visible; }`);

  // Add background
  svg
    .append("rect")
    .attr("width", "100%")
    .attr("height", "100%")
    .attr("fill", "black");

  // Create main plot area similar to dayview
  const plot = svg
    .append("svg")
    .attr("class", "plot")
    .attr("x", margin.left)
    .attr("y", margin.top)
    .attr("width", plotWidth)
    .attr("height", plotHeight);

  // Calculate width ratios based on history and now-width
  let timelineWidthRatio = 0;
  let statePanelWidthRatio = 1;

  if (historyMinutes > 0) {
    // Calculate ratio of current state width based on duration
    const totalMinutes = historyMinutes + nowWidthMinutes;
    statePanelWidthRatio = nowWidthMinutes / totalMinutes;
    timelineWidthRatio = 1 - statePanelWidthRatio;
  }

  // Only create history panel if history is requested
  let dataWidth = 0;

  if (timelineWidthRatio > 0) {
    // Add data area for history
    dataWidth = plotWidth * timelineWidthRatio;

    const dataArea = plot
      .append("svg")
      .attr("class", "dataArea")
      .attr("width", dataWidth)
      .attr("height", dataHeight)
      .attr("x", 0);

    // Get data using allFieldsSvg for timeline
    const timelineFieldsSvg = await allFieldsSvg({
      db,
      startUtc,
      endUtc,
      width: dataWidth,
      height: dataHeight,
    });

    if (timelineFieldsSvg) {
      dataArea.append(() => timelineFieldsSvg);
    }
  }

  // Create current state panel
  const stateWidth = plotWidth * statePanelWidthRatio;
  const stateX = dataWidth || 0;

  const stateArea = plot
    .append("svg")
    .attr("class", "stateArea")
    .attr("width", stateWidth)
    .attr("height", dataHeight)
    .attr("x", stateX);

  // Get current state data using allFieldsSvg with a tiny time window
  const sliverBeforeEndUtc = endTime.minus({ milliseconds: 1 }).toUTC().toISO(); // Small window to get current state

  const stateFieldsSvg = await allFieldsSvg({
    db,
    startUtc: sliverBeforeEndUtc,
    endUtc: endUtc,
    width: stateWidth,
    height: dataHeight,
  });

  if (stateFieldsSvg) {
    stateArea.append(() => stateFieldsSvg);
  }

  // Add border around state panel
  stateArea
    .append("rect")
    .attr("x", 0)
    .attr("y", 0)
    .attr("width", stateWidth)
    .attr("height", dataHeight)
    .attr("fill", "none")
    .attr("stroke", "#ffcc00")
    .attr("stroke-width", 2)
    .attr("stroke-opacity", 0.7);

  // Only add timeline elements if history is requested
  if (timelineWidthRatio > 0) {
    // Create time scale with current time at right edge of timeline
    const timeScale = d3
      .scaleTime()
      .domain([startTime.toJSDate(), endTime.toJSDate()])
      .range([0, dataWidth]);

    // Add a vertical line indicating the current time
    plot
      .append("line")
      .attr("class", "current-time-line")
      .attr("x1", timeScale(endTime.toJSDate()))
      .attr("x2", timeScale(endTime.toJSDate()))
      .attr("y1", 0)
      .attr("y2", dataHeight)
      .attr("stroke", "#ffcc00")
      .attr("stroke-width", 2)
      .attr("stroke-dasharray", "4,2");

    // Calculate appropriate tick values based on history duration
    const tickValues: Date[] = [endTime.toJSDate()];

    // Add tick marks at appropriate intervals
    if (historyMinutes <= 15) {
      // For short durations, use 5-minute intervals
      for (let i = 5; i <= historyMinutes; i += 5) {
        tickValues.push(endTime.minus({ minutes: i }).toJSDate());
      }
    } else if (historyMinutes <= 60) {
      // For medium durations, use 15-minute intervals
      for (let i = 15; i <= historyMinutes; i += 15) {
        tickValues.push(endTime.minus({ minutes: i }).toJSDate());
      }
    } else {
      // For longer durations, use 30-minute or 1-hour intervals
      const interval = historyMinutes <= 180 ? 30 : 60;
      for (let i = interval; i <= historyMinutes; i += interval) {
        tickValues.push(endTime.minus({ minutes: i }).toJSDate());
      }
    }

    // Add the start time if it's not already included
    if (historyMinutes % 5 !== 0) {
      tickValues.push(startTime.toJSDate());
    }

    // Sort tick values in ascending order
    tickValues.sort((a, b) => a.getTime() - b.getTime());

    const axis = d3.axisBottom(timeScale as any);
    const timeAxis = plot
      .append("g")
      .attr("transform", `translate(0, ${dataHeight})`)
      .call(
        axis.tickValues(tickValues as any).tickFormat((d) => {
          const date = d as Date;
          const diffMinutes = Math.round(
            (date.getTime() - endTime.toJSDate().getTime()) / 60000,
          );
          return diffMinutes === 0
            ? `${endTime.hour}:${endTime.minute.toString().padStart(2, "0")}`
            : `${diffMinutes}m`;
        }),
      );

    timeAxis.selectAll("text").attr("fill", "white");
    timeAxis.selectAll("line").attr("stroke", "white");
    timeAxis.selectAll("path").attr("stroke", "white");

    // Add vertical grid lines at the tick positions
    plot
      .append("g")
      .selectAll(".gridline")
      .data(tickValues)
      .enter()
      .append("line")
      .attr("class", "gridline")
      .attr("x1", (d) => timeScale(d))
      .attr("x2", (d) => timeScale(d))
      .attr("y1", 0)
      .attr("y2", dataHeight)
      .attr("stroke", "white")
      .attr("stroke-opacity", 0.4);
  }

  const outputFile = args.outputFile;
  const prettySvg = xmlFormatter(svg.node()!.outerHTML);

  if (outputFile === undefined) {
    return prettySvg;
  }
  if (outputFile.endsWith(".svg")) {
    fs.writeFileSync(outputFile, xmlDeclaration + prettySvg);
    return prettySvg;
  } else if (outputFile.endsWith(".png")) {
    await sharp(Buffer.from(xmlDeclaration + prettySvg))
      .png()
      .toFile(outputFile);
    return prettySvg;
  } else if (outputFile.endsWith(".html")) {
    const prettyHtml = xmlFormatter(document.documentElement.outerHTML);
    fs.writeFileSync(outputFile, prettyHtml);
    return prettySvg;
  } else {
    throw new Error("output file must have an .svg, .png, or .html");
  }
}

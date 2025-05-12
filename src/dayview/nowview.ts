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
const DEFAULT_NOW_WIDTH_MINUTES = 5;

export async function nowview(args: NowviewCmdArgs): Promise<string> {
  const db = connectDb(args);

  let historyMinutes = 0;
  if (args.history) {
    try {
      const historyDuration = parseDurationStr({ durationStr: args.history });
      historyMinutes = Math.abs(historyDuration.as("minutes"));
    } catch {
      console.error(
        `Invalid history format: ${args.history}, defaulting to no history`,
      );
    }
  }
  let nowWidthMinutes = DEFAULT_NOW_WIDTH_MINUTES;
  if (args.nowWidth) {
    try {
      const nowWidthDuration = parseDurationStr({ durationStr: args.nowWidth });
      nowWidthMinutes = Math.abs(nowWidthDuration.as("minutes"));
    } catch {
      console.error(
        `Invalid now-width format: ${args.nowWidth}, using default of ${nowWidthMinutes} minutes`,
      );
    }
  }

  // Scale default width based on history size
  let defaultWidth = 100;
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

  const width = args.width ?? defaultWidth;
  const height = args.height ?? DEFAULT_HEIGHT;
  const margin = { top: 30, right: 2, bottom: 15, left: 2 };
  const timeAxisHeight = args.timeAxisHeight ?? DEFAULT_TIME_AXIS_HEIGHT;

  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const dataHeight = plotHeight - timeAxisHeight;

  const endTime = now() as DateTime<true>;

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

  const defs = svg.append("defs");
  defs.append("style").text(`svg { overflow: visible; }`);

  // Add background
  svg
    .append("rect")
    .attr("width", "100%")
    .attr("height", "100%")
    .attr("fill", "black");

  const plot = svg
    .append("svg")
    .attr("class", "plot")
    .attr("x", margin.left)
    .attr("y", margin.top)
    .attr("width", plotWidth)
    .attr("height", plotHeight);

  let timelineWidthRatio = 0;
  let nowPanelWidthRatio = 1;

  if (historyMinutes > 0) {
    const totalMinutes = historyMinutes + nowWidthMinutes;
    nowPanelWidthRatio = nowWidthMinutes / totalMinutes;
    timelineWidthRatio = 1 - nowPanelWidthRatio;
  }

  let dataWidth = 0;
  const nowWidth = plotWidth * nowPanelWidthRatio;

  if (timelineWidthRatio > 0) {
    dataWidth = plotWidth * timelineWidthRatio;
  }
  const nowX = dataWidth;

  const nowArea = plot
    .append("svg")
    .attr("class", "nowArea")
    .attr("width", nowWidth)
    .attr("height", dataHeight)
    .attr("x", nowX);

  const sliverBeforeEndUtc = endTime.minus({ milliseconds: 1 }).toUTC().toISO();
  const nowFieldsSvg = await allFieldsSvg({
    db,
    startUtc: sliverBeforeEndUtc,
    endUtc: endUtc,
    width: nowWidth,
    height: dataHeight,
  });

  if (nowFieldsSvg) {
    nowArea.append(() => nowFieldsSvg);
  }

  if (timelineWidthRatio === 0) {
    // Add timestamp
    plot
      .append("text")
      .attr("class", "current-time-text")
      .attr("x", nowWidth / 2)
      .attr("y", dataHeight + 16)
      .attr("text-anchor", "middle")
      .attr("fill", "white")
      .attr("font-size", "16px")
      .attr("font-weight", "bold")
      .text(`${endTime.hour}:${endTime.minute.toString().padStart(2, "0")}`);
  }

  // Create history panel if requested
  if (timelineWidthRatio > 0) {
    const dataArea = plot
      .append("svg")
      .attr("class", "dataArea")
      .attr("width", dataWidth)
      .attr("height", dataHeight)
      .attr("x", 0);

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

  if (timelineWidthRatio > 0) {
    const timeScale = d3
      .scaleTime()
      .domain([startTime.toJSDate(), endTime.toJSDate()])
      .range([0, dataWidth]);

    // Create vertical line from top to time axis
    plot
      .append("line")
      .attr("class", "current-time-line-vertical")
      .attr("x1", timeScale(endTime.toJSDate()))
      .attr("x2", timeScale(endTime.toJSDate()))
      .attr("y1", 0)
      .attr("y2", dataHeight + 22) // Extend below timestamp text
      .attr("stroke", "#ffcc00")
      .attr("stroke-width", 2)
      .attr("stroke-dasharray", "4,2");

    // Create horizontal underline beneath timestamp and now panel
    plot
      .append("line")
      .attr("class", "current-time-line-horizontal")
      .attr("x1", timeScale(endTime.toJSDate()))
      .attr("x2", nowX + nowWidth) // Full width of now panel
      .attr("y1", dataHeight + 22) // Position fully below the timestamp text
      .attr("y2", dataHeight + 22)
      .attr("stroke", "#ffcc00")
      .attr("stroke-width", 2)
      .attr("stroke-dasharray", "4,2");

    // Calculate appropriate tick values based on history duration
    const tickValues: Date[] = [];

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
          return `${diffMinutes}m`;
        }),
      );

    // Style time axis
    timeAxis.selectAll("text").attr("fill", "white");
    timeAxis.selectAll("line").attr("stroke", "white");
    timeAxis.selectAll("path").attr("stroke", "white");

    // Add a dedicated current time label centered under the now panel
    plot
      .append("text")
      .attr("class", "current-time-text")
      .attr("x", nowX + nowWidth / 2)
      .attr("y", dataHeight + 16)
      .attr("text-anchor", "middle")
      .attr("fill", "white")
      .attr("font-size", "16px")
      .attr("font-weight", "bold")
      .text(`${endTime.hour}:${endTime.minute.toString().padStart(2, "0")}`);

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

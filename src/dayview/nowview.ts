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

const DEFAULT_WIDTH = 400;
const DEFAULT_HEIGHT = 300;
const DEFAULT_TIME_AXIS_HEIGHT = 15;
const TIMELINE_WIDTH_RATIO = 0.6;
const STATE_PANEL_WIDTH_RATIO = 0.4;

export async function nowview(args: NowviewCmdArgs): Promise<string> {
  const db = connectDb(args);

  // Setup dimensions
  const width = args.width ?? DEFAULT_WIDTH;
  const height = args.height ?? DEFAULT_HEIGHT;
  const margin = { top: 30, right: 2, bottom: 15, left: 2 };
  const timeAxisHeight = args.timeAxisHeight ?? DEFAULT_TIME_AXIS_HEIGHT;

  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const dataHeight = plotHeight - timeAxisHeight;

  const endTime = now() as DateTime<true>;
  const startTime = endTime.minus({ minutes: 15 });
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

  // Add data area
  const dataWidth = plotWidth * TIMELINE_WIDTH_RATIO;

  const dataArea = plot
    .append("svg")
    .attr("class", "dataArea")
    .attr("width", dataWidth)
    .attr("height", dataHeight)
    .attr("x", 0);

  // Get data using allFieldsSvg for timeline (past 15 minutes)
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

  // Create selected state panel
  const stateWidth = plotWidth * STATE_PANEL_WIDTH_RATIO;
  const stateArea = plot
    .append("svg")
    .attr("class", "stateArea")
    .attr("width", stateWidth)
    .attr("height", dataHeight)
    .attr("x", dataWidth);

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

  const tickValues = [
    endTime.toJSDate(),
    endTime.minus({ minutes: 5 }).toJSDate(),
    endTime.minus({ minutes: 10 }).toJSDate(),
    endTime.minus({ minutes: 15 }).toJSDate(),
  ];

  const timeAxis = plot
    .append("g")
    .attr("transform", `translate(0, ${dataHeight})`)
    .call(
      d3
        .axisBottom(timeScale)
        .tickValues(tickValues)
        .tickFormat((d) => {
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

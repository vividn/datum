import * as d3 from "d3";
import { DateTime } from "luxon";
import { connectDb } from "../auth/connectDb";
import { NowviewCmdArgs } from "../commands/nowviewCmd";
import { domdoc } from "./domdoc";
import { getNowviewScript } from "./nowviewScript"; // TODO: Complete this
import { allFieldsSvg } from "../dayview/allFieldsSvg";

const DEFAULT_WIDTH = 400;
const DEFAULT_HEIGHT = 300;
const DEFAULT_TIME_AXIS_HEIGHT = 15;
const TIMELINE_WIDTH_RATIO = 0.6;
const STATE_PANEL_WIDTH_RATIO = 0.4;

// Extract time-related calculations into a helper function
function calculateTimeRange(timeShiftMinutes: number) {
  const viewportEnd = DateTime.now().minus({ minutes: timeShiftMinutes });
  const viewportStart = viewportEnd.minus({ minutes: 15 });
  return {
    viewportEnd,
    viewportStart,
    startUtc: viewportStart.toUTC().toISO(),
    endUtc: viewportEnd.toUTC().toISO(),
  };
}

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

  const document = domdoc();
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

  // Get timeshift and calculate time range
  const timeShiftMinutes = args.timeshift ? parseInt(args.timeshift, 10) : 0;
  const { viewportEnd, viewportStart, startUtc, endUtc } =
    calculateTimeRange(timeShiftMinutes);

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
  const stateEndUtc = viewportEnd.toUTC().toISO();
  const stateStartUtc = viewportEnd.minus({ milliseconds: 1 }).toUTC().toISO(); // Small window to get current state

  const stateFieldsSvg = await allFieldsSvg({
    db,
    startUtc: stateStartUtc,
    endUtc: stateEndUtc,
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
    .domain([viewportStart.toJSDate(), viewportEnd.toJSDate()])
    .range([0, dataWidth]);

  // Add a vertical line indicating the current time
  plot
    .append("line")
    .attr("class", "current-time-line")
    .attr("x1", timeScale(viewportEnd.toJSDate()))
    .attr("x2", timeScale(viewportEnd.toJSDate()))
    .attr("y1", 0)
    .attr("y2", dataHeight)
    .attr("stroke", "#ffcc00")
    .attr("stroke-width", 2)
    .attr("stroke-dasharray", "4,2");

  const tickValues = [
    viewportEnd.toJSDate(),
    viewportEnd.minus({ minutes: 5 }).toJSDate(),
    viewportEnd.minus({ minutes: 10 }).toJSDate(),
    viewportEnd.minus({ minutes: 15 }).toJSDate(),
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
            (date.getTime() - viewportEnd.toJSDate().getTime()) / 60000,
          );
          return diffMinutes === 0
            ? `${viewportEnd.hour}:${viewportEnd.minute.toString().padStart(2, "0")}`
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

  // Add keyboard navigation script
  const script = document.createElement("script");
  script.textContent = getNowviewScript();
  document.body.appendChild(script);

  return document.documentElement.outerHTML;
}

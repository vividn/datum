import * as d3 from "d3";
import { DateTime } from "luxon";
import { connectDb } from "../auth/connectDb";
import { NowviewCmdArgs } from "../commands/nowviewCmd";
import { domdoc } from "./domdoc";
import { occurredFields } from "../field/occurredFields";
import { getFieldSpec } from "../field/mySpecs";
import md5 from "md5";
import { getNowviewScript } from "./nowviewScript"; // TODO: Complete this
import { allFieldsSvg } from "../dayview/allFieldsSvg";

// Extract time-related calculations into a helper function
function calculateTimeRange(timeShiftMinutes: number) {
  const now = DateTime.now().minus({ minutes: timeShiftMinutes });
  const fifteenMinutesAgo = now.minus({ minutes: 15 });
  return {
    now,
    fifteenMinutesAgo,
    startUtc: fifteenMinutesAgo.toUTC().toISO(),
    endUtc: now.toUTC().toISO(),
  };
}

// Extract field layout calculations into a helper function
function calculateFieldLayout(innerHeight: number, fieldCount: number) {
  const spacing = (innerHeight * 0.1) / (fieldCount + 1);
  const fieldHeight = (innerHeight - spacing * (fieldCount + 1)) / fieldCount;
  return { spacing, fieldHeight };
}

export async function nowview(args: NowviewCmdArgs): Promise<string> {
  const db = connectDb(args);

  // Setup dimensions
  const width = args.width ?? 400;
  const height = args.height ?? 300; // Increased default height
  const margin = { top: 30, right: 2, bottom: 15, left: 2 };
  const timeAxisHeight = args.timeAxisHeight ?? 15;

  const innerWidth = width - margin.left - margin.right;
  const innerHeight = height - margin.top - margin.bottom;
  const plotHeight = innerHeight;
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
  const timeShiftMinutes = args.timeshift ? parseInt(args.timeshift) : 0;
  const { now, fifteenMinutesAgo, startUtc, endUtc } =
    calculateTimeRange(timeShiftMinutes);

  // Get and sort fields
  const allFields = await occurredFields(db);
  const sortedFields = allFields
    .map((field) => ({
      field,
      specY:
        getFieldSpec(field).y ??
        parseInt(md5(field).slice(0, 8), 16) / Math.pow(2, 32),
    }))
    .sort((a, b) => a.specY - b.specY);

  // Calculate block dimensions more like dayview
  const blockSpacing = Math.max(2, dataHeight * 0.02); // 2% of data height for spacing
  const blockHeight = Math.max(
    15, // minimum height
    (dataHeight - blockSpacing * (sortedFields.length + 1)) /
      sortedFields.length,
  );

  // Create main plot area similar to dayview
  const plot = svg
    .append("svg")
    .attr("class", "plot")
    .attr("x", margin.left)
    .attr("y", margin.top)
    .attr("width", innerWidth)
    .attr("height", plotHeight);

  // Add data area
  const dataWidth = innerWidth * 0.6; // 60% for timeline
  const labelWidth = 60;

  const dataArea = plot
    .append("svg")
    .attr("class", "dataArea")
    .attr("width", dataWidth)
    .attr("height", dataHeight)
    .attr("x", labelWidth);

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

  // Create selected state panel (remaining 30% width)
  const stateWidth = innerWidth * 0.3;
  const stateArea = plot
    .append("svg")
    .attr("class", "stateArea")
    .attr("width", stateWidth)
    .attr("height", dataHeight)
    .attr("x", labelWidth + dataWidth);

  // Get current state data using allFieldsSvg with a tiny time window
  const stateEndUtc = now.toUTC().toISO();
  const stateStartUtc = now.minus({ milliseconds: 1 }).toUTC().toISO(); // Small window to get current state

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
    .domain([fifteenMinutesAgo.toJSDate(), now.toJSDate()])
    .range([0, dataWidth]);

  // Add a vertical line indicating the current time
  plot
    .append("line")
    .attr("class", "current-time-line")
    .attr("x1", labelWidth + timeScale(now.toJSDate()))
    .attr("x2", labelWidth + timeScale(now.toJSDate()))
    .attr("y1", 0)
    .attr("y2", dataHeight)
    .attr("stroke", "#ffcc00")
    .attr("stroke-width", 2)
    .attr("stroke-dasharray", "4,2");

  // Create custom tick values for the time axis
  const tickValues = [
    now.toJSDate(),
    now.minus({ minutes: 5 }).toJSDate(),
    now.minus({ minutes: 10 }).toJSDate(),
    now.minus({ minutes: 15 }).toJSDate(),
  ];

  const timeAxis = plot
    .append("g")
    .attr("transform", `translate(${labelWidth}, ${dataHeight})`)
    .call(
      d3
        .axisBottom(timeScale)
        .tickValues(tickValues)
        .tickFormat((d, i) => {
          const diffMinutes = Math.round(
            (d.getTime() - now.toJSDate().getTime()) / 60000,
          );
          return diffMinutes === 0
            ? `${now.hour}:${now.minute.toString().padStart(2, "0")}`
            : `${diffMinutes}m`;
        }),
    );

  timeAxis.selectAll("text").attr("fill", "white");
  timeAxis.selectAll("line").attr("stroke", "white");
  timeAxis.selectAll("path").attr("stroke", "white");

  // Add vertical grid lines at the tick positions
  const gridLines = plot
    .append("g")
    .attr("transform", `translate(${labelWidth}, 0)`)
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

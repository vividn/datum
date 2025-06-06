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
const DEFAULT_MARGIN = 2;

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
  const margin = args.margin ?? DEFAULT_MARGIN;
  const timeAxisHeight = args.timeAxisHeight ?? DEFAULT_TIME_AXIS_HEIGHT;

  const plotWidth = width - margin * 2;
  const plotHeight = height - margin * 2;
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

  const _background = svg
    .append("rect")
    .attr("width", "100%")
    .attr("height", "100%")
    .attr("fill", "black");

  const plot = svg
    .append("svg")
    .attr("class", "plot")
    .attr("x", margin)
    .attr("y", margin)
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
    // For standalone mode, we'll just consider whether the total width is small
    const isSmallWidth = plotWidth < 50;
    const textX = nowWidth / 2;

    // For small widths, add a background to improve readability
    if (isSmallWidth) {
      // Add background for text
      plot
        .append("rect")
        .attr("x", textX - 22) // Provide padding around text
        .attr("y", dataHeight + 1) // Just below data area
        .attr("width", 44) // Fixed width to accommodate time text
        .attr("height", timeAxisHeight - 1)
        .attr("fill", "black")
        .attr("stroke", "#ffcc00")
        .attr("stroke-width", 1)
        .attr("stroke-dasharray", "2,1");
    }

    plot
      .append("text")
      .attr("class", "current-time-text")
      .attr("x", textX)
      .attr("y", dataHeight + timeAxisHeight * 0.65) // Position lower to better fill the timeAxisHeight
      .attr("text-anchor", "middle")
      .attr("dominant-baseline", "middle")
      .attr("fill", "white")
      .attr(
        "font-size",
        `${Math.min(timeAxisHeight * 1.2, timeAxisHeight + 2)}px`,
      ) // Increase font size
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
      .attr("y2", dataHeight + timeAxisHeight) // Extend to bottom of time axis
      .attr("stroke", "#ffcc00")
      .attr("stroke-width", 2)
      .attr("stroke-dasharray", "4,2");

    // Create horizontal line at the top of time axis
    plot
      .append("line")
      .attr("class", "current-time-line-horizontal")
      .attr("x1", timeScale(endTime.toJSDate()))
      .attr("x2", nowX + nowWidth) // Full width of now panel
      .attr("y1", dataHeight) // Place at bottom of data area (top of time axis)
      .attr("y2", dataHeight)
      .attr("stroke", "#ffcc00")
      .attr("stroke-width", 2)
      .attr("stroke-dasharray", "4,2");

    // Configure tick intervals based on history duration
    const getTickConfig = (historyMinutes: number) => {
      if (historyMinutes <= 15) {
        return { majorInterval: 5, minorInterval: null };
      } else if (historyMinutes <= 60) {
        return { majorInterval: 15, minorInterval: 5 };
      } else if (historyMinutes <= 180) {
        return { majorInterval: 30, minorInterval: 10 };
      } else {
        return { majorInterval: 60, minorInterval: 15 };
      }
    };

    const tickConfig = getTickConfig(historyMinutes);
    const majorTickValues: Date[] = [];
    const minorTickValues: Date[] = [];

    // Generate tick values
    for (
      let i = tickConfig.majorInterval;
      i <= historyMinutes;
      i += tickConfig.majorInterval
    ) {
      majorTickValues.push(endTime.minus({ minutes: i }).toJSDate());
    }

    if (tickConfig.minorInterval) {
      for (
        let i = tickConfig.minorInterval;
        i <= historyMinutes;
        i += tickConfig.minorInterval
      ) {
        if (i % tickConfig.majorInterval !== 0) {
          // Skip major tick positions
          minorTickValues.push(endTime.minus({ minutes: i }).toJSDate());
        }
      }
    }

    // Sort tick values in ascending order
    const tickValues = majorTickValues.concat(minorTickValues);
    tickValues.sort((a, b) => a.getTime() - b.getTime());
    majorTickValues.sort((a, b) => a.getTime() - b.getTime());
    minorTickValues.sort((a, b) => a.getTime() - b.getTime());

    const axis = d3.axisBottom(timeScale as any);
    const timeAxis = plot
      .append("g")
      .attr("transform", `translate(0, ${dataHeight})`)
      .call(
        axis.tickValues(majorTickValues as any).tickFormat((d) => {
          const date = d as Date;
          const diffMinutes = Math.round(
            (date.getTime() - endTime.toJSDate().getTime()) / 60000,
          );
          return `${diffMinutes}m`;
        }),
      );

    // Style time axis
    timeAxis.selectAll("path").attr("stroke", "white");
    timeAxis.selectAll("line").attr("stroke", "white");
    if (timeAxisHeight <= 6) {
      timeAxis.selectAll("text").remove();
    } else {
      timeAxis
        .selectAll("text")
        .attr("stroke", "white")
        .attr("fill", "white")
        .style("font-size", `${timeAxisHeight - 6}px`)
        .each(function (d, i, _nodes) {
          // Adjust the position of the leftmost tick label to prevent truncation
          if (i === 0) {
            const textElem = d3.select(this);
            const currentX = parseFloat(textElem.attr("x") || "0");
            // If the label is very close to the left edge, shift it right
            if (Math.abs(currentX) < 5) {
              textElem.attr("text-anchor", "start").attr("x", 3);
            }
          }
        });
    }

    // Add a dedicated current time label
    // When nowWidth is small, center the timestamp on the vertical now line
    // When nowWidth is large enough, center it within the now panel
    const isNowWidthSmall = nowWidth < 40; // Define threshold for small width
    const textX = isNowWidthSmall
      ? timeScale(endTime.toJSDate())
      : nowX + nowWidth / 2;

    // For small widths, add a background to improve readability
    if (isNowWidthSmall) {
      // Add background for text
      plot
        .append("rect")
        .attr("x", textX - 22) // Provide padding around text
        .attr("y", dataHeight + 1) // Just below the horizontal line
        .attr("width", 44) // Fixed width to accommodate time text
        .attr("height", timeAxisHeight - 1)
        .attr("fill", "black");
    }

    plot
      .append("text")
      .attr("class", "current-time-text")
      .attr("x", textX)
      .attr("y", dataHeight + timeAxisHeight * 0.65) // Position lower to better fill the timeAxisHeight
      .attr("text-anchor", "middle")
      .attr("dominant-baseline", "middle")
      .attr("fill", "white")
      .attr(
        "font-size",
        `${Math.min(timeAxisHeight * 1.2, timeAxisHeight + 2)}px`,
      ) // Increase font size
      .attr("font-weight", "bold")
      .text(`${endTime.hour}:${endTime.minute.toString().padStart(2, "0")}`);

    // Add vertical grid lines at the tick positions
    // Major ticks
    plot
      .append("g")
      .selectAll(".gridline-major")
      .data(majorTickValues)
      .enter()
      .append("line")
      .attr("class", "gridline-major")
      .attr("x1", (d) => timeScale(d))
      .attr("x2", (d) => timeScale(d))
      .attr("y1", 0)
      .attr("y2", dataHeight)
      .attr("stroke", "white")
      .attr("stroke-opacity", 0.4);

    // Minor ticks
    plot
      .append("g")
      .selectAll(".gridline-minor")
      .data(minorTickValues)
      .enter()
      .append("line")
      .attr("class", "gridline-minor")
      .attr("x1", (d) => timeScale(d))
      .attr("x2", (d) => timeScale(d))
      .attr("y1", 0)
      .attr("y2", dataHeight)
      .attr("stroke", "white")
      .attr("stroke-opacity", 0.1);
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

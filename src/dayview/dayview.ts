import fs from "fs";
import * as d3 from "d3";
import { JSDOM } from "jsdom";
import md5 from "md5";
import { DayviewCmdArgs } from "../commands/dayviewCmd";
import { connectDb } from "../auth/connectDb";
import { occurredFields } from "../field/occurredFields";
import { DateTime } from "luxon";
import { fieldSvgBlocks } from "./fieldSvgBlocks";
import { DAYVIEW_SPANS } from "../field/tempExampleSpans";

function getSpan(field: string): [number, number] {
  const customSpan = DAYVIEW_SPANS[field];
  if (customSpan) {
    return [customSpan[0], customSpan[1] - customSpan[0]];
  }

  const hash = md5(field);
  const y1 = parseInt(hash.slice(0, 8), 16) / Math.pow(2, 32);
  return [y1, 0.02];
}

export async function dayview(args: DayviewCmdArgs): Promise<void> {
  const db = connectDb(args);

  const startUtc = DateTime.local().startOf("day").toUTC().toISO();
  const endUtc = DateTime.local()
    .startOf("day")
    .plus({ day: 1 })
    .toUTC()
    .toISO();

  const document = new JSDOM().window.document;

  const width = 1850;
  const height = 700;
  const margin = 10;

  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("width", width)
    .attr("height", height);

  const _background = svg
    .append("rect")
    .attr("width", "100%")
    .attr("height", "100%")
    .attr("fill", "black");

  const plotWidth = width - 2 * margin;
  const plotHeight = height - 2 * margin;

  const plot = svg
    .append("svg")
    .attr("class", "plot")
    .attr("x", margin)
    .attr("y", margin)
    .attr("width", plotWidth)
    .attr("height", plotHeight);

  // dataArea
  //   .append("rect")
  //   .attr("width", dataWidth)
  //   .attr("height", dataHeight)
  //   .attr("fill", "orange");

  const timeScale = d3
    .scaleTime()
    .domain([new Date(startUtc), new Date(endUtc)])
    .range([0, plotWidth]);

  const xAxis = d3.axisBottom(timeScale).ticks(d3.timeHour.every(3), "%H");

  const axisHeight = 20;
  const axis = plot
    .append("g")
    .attr("transform", `translate(0, ${plotHeight - axisHeight})`);
  // .attr("fill", "white")
  // .attr("stroke", "white");
  // .attr("y", plotHeight - axisHeight)
  // .attr("height", axisHeight)
  // .attr("width", plotWidth)
  // .attr("viewBox", [0, 0, plotWidth, axisHeight])
  // .attr("preserveAspectRatio", "xMinYMid meet");

  axis.call(xAxis).selectAll("path").attr("stroke", "white");
  axis.call(xAxis).selectAll("line").attr("stroke", "white");
  axis.call(xAxis).selectAll("text").attr("stroke", "white");
  // .selectAll("text")
  // .attr("fill", "white");
  // .attr("font-size", axisHeight / 2)
  // axis.call(xAxis);
  //

  const dataWidth = plotWidth;
  const dataHeight = plotHeight - axisHeight;

  const dataArea = plot
    .append("svg")
    .attr("class", "dataArea")
    .attr("width", dataWidth)
    .attr("height", dataHeight);

  const allFields = await occurredFields(db);
  const sortableGroups = await Promise.all(
    allFields.map(async (field) => {
      const [p1, pHeight] = getSpan(field);
      const y1 = p1 * dataHeight;
      const fieldHeight = dataHeight * pHeight;
      const fieldSvg = await fieldSvgBlocks({
        db,
        field,
        startUtc,
        endUtc,
        width: dataWidth,
        height: fieldHeight,
      });
      return { field, y1, fieldHeight, svg: fieldSvg };
    }),
  );

  const sortedGroups = sortableGroups.sort((a, b) => a.y1 - b.y1);
  sortedGroups.forEach((field) => {
    if (field.svg !== null) {
      const fieldSvg = dataArea.append(() => field.svg);
      fieldSvg
        .attr("x", 0)
        .attr("y", field.y1)
        .attr("width", dataWidth)
        .attr("height", field.fieldHeight);
    }
  });

  // return svg.node()!.outerHTML;
  const dir = "/tmp/";
  fs.writeFileSync(dir + "dayview.svg", svg.node()!.outerHTML);

  // auto refresh html
  const meta = document.createElement("meta");
  meta.setAttribute("http-equiv", "refresh");
  meta.setAttribute("content", "1");
  document.head.append(meta);
  fs.writeFileSync(dir + "dayview.html", document.documentElement.outerHTML);
}

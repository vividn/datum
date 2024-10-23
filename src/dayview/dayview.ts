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
    return customSpan;
  }

  const hash = md5(field);
  const y1 = (parseInt(hash.slice(0, 8), 16) / Math.pow(2, 32)) * 0.1;
  return [y1, y1 + 0.005];
}

function md5Span(field: string, limit: [number, number]) {
  // use md5 to generate 2 random numbers between 0 and 1
  const hash = md5(field);
  const y1 = parseInt(hash.slice(0, 8), 16) / Math.pow(2, 32);
  const y2 = parseInt(hash.slice(8, 16), 16) / Math.pow(2, 32);
  // const y2 = y1 + 0.04;
  return [y1, y2].sort();
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

  const width = 1500;
  const height = 800;
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
    .range([0, 10000]);

  const xAxis = d3.axisBottom(timeScale).ticks(d3.timeHour.every(3), "%H");

  const axis = plot
    .append("svg")
    .attr("y", "98%")
    .attr("height", "2%")
    .attr("width", "100%")
    .attr("viewBox", [0, 0, 10000, 20])
    .attr("preserveAspectRatio", "xMinYMid meet");

  axis
    .append("g")
    .call(xAxis)
    .selectAll("text")
    .attr("font-size", "40px")
    .attr("fill", "white");
  // axis.call(xAxis);

  const dataArea = plot
    .append("svg")
    .attr("class", "dataArea")
    .attr("width", "100%")
    .attr("height", "98%")
    .attr("viewBox", [0, 0, 1, 1])
    .attr("preserveAspectRatio", "none");

  const allFields = await occurredFields(db);
  const sortableGroups = await Promise.all(
    allFields.map(async (field) => {
      const [y1, y2] = getSpan(field);
      const g = await fieldSvgBlocks({ db, field, startUtc, endUtc });
      return { field, y1, y2, g };
    }),
  );

  const sortedGroups = sortableGroups.sort((a, b) => a.y1 - b.y1);
  sortedGroups.forEach((group) => {
    const y = group.y1;
    const fieldHeight = group.y2 - group.y1;
    if (group.g === null) {
      return;
    }
    const g = dataArea.append(() => group.g);
    g.attr("y", y).attr("height", fieldHeight);
    g.attr("x", 0).attr("width", "100%");
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

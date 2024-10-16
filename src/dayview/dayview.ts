import fs from "fs";
import * as d3 from "d3";
import { JSDOM } from "jsdom";
import md5 from "md5";
import { DayviewCmdArgs } from "../commands/dayviewCmd";
import { connectDb } from "../auth/connectDb";
import { occurredFields } from "../field/occurredFields";
import { DateTime } from "luxon";
import { fieldSvgBlocks } from "./fieldSvgBlocks";

function md5Span(field: string) {
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
  const endUtc = DateTime.local().endOf("day").toUTC().toISO();

  const allFields = await occurredFields(db);
  const sortableGroups = await Promise.all(
    allFields.map(async (field) => {
      const [y1, y2] = md5Span(field);
      const g = await fieldSvgBlocks({ db, field, startUtc, endUtc });
      return { field, y1, y2, g };
    }),
  );

  const sortedGroups = sortableGroups.sort((a, b) => a.y1 - b.y1);

  const document = new JSDOM().window.document;

  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("min-height", "200px")
    .attr("min-width", "500px");
  // .attr("width", "100%")
  // .attr("height", "100%");

  const _background = svg
    .append("rect")
    .attr("width", "100%")
    .attr("height", "100%")
    .attr("fill", "lightgray");

  const marginPercent = 1;
  const plot = svg
    .append("svg")
    .attr("class", "plot")
    .attr("x", marginPercent + "%")
    .attr("y", marginPercent + "%")
    .attr("width", 100 - 2 * marginPercent + "%")
    .attr("height", 100 - 2 * marginPercent + "%");

  // dataArea
  //   .append("rect")
  //   .attr("width", dataWidth)
  //   .attr("height", dataHeight)
  //   .attr("fill", "orange");

  const timeScale = d3
    .scaleTime()
    .domain([new Date(startUtc), new Date(endUtc)])
    .range([0, 1]);

  const xAxis = d3.axisBottom(timeScale);

  plot.append("g").attr("width", "100%").call(xAxis);

  const dataArea = plot
    .append("svg")
    .attr("class", "dataArea")
    .attr("width", "100%")
    .attr("height", "100%")
    .attr("viewBox", [0, 0, 1, 1])
    .attr("preserveAspectRatio", "none");

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
  fs.writeFileSync("dayview.svg", svg.node()!.outerHTML);
  fs.writeFileSync("dayview.html", document.documentElement.outerHTML);
}
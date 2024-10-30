import fs from "fs";
import * as d3 from "d3";
import { DayviewCmdArgs } from "../commands/dayviewCmd";
import { connectDb } from "../auth/connectDb";
import { DateTime } from "luxon";
import { singleDay } from "./singleday";
import { domdoc } from "./domdoc";

export async function dayview(args: DayviewCmdArgs): Promise<void> {
  const db = connectDb(args);
  const nDays = 7;
  const endDate = DateTime.local();

  const document = domdoc();

  const width = 1850;
  const height = 700;
  const margin = 10;
  const interdayMargin = 15;

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
    .scaleUtc()
    .domain([new Date("2024-10-31"), new Date("2024-11-01")]) // 🎃
    .range([0, plotWidth]);

  const xAxis = d3.axisBottom(timeScale).ticks(d3.timeHour.every(1), "%H");

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

  // array of the last seven iso dates
  const days = Array.from({ length: nDays }, (_, i) => {
    return endDate.minus({ days: nDays - 1 - i }).toISODate();
  });
  const dayHeight = (dataHeight - interdayMargin * (nDays - 1)) / nDays;

  await Promise.all(
    days.map(async (date, i) => {
      const y = i * (dayHeight + interdayMargin);
      const daySvg = await singleDay({
        db,
        date,
        dataWidth,
        height: dayHeight,
        labelWidth: 0,
      });

      dataArea.append(() => daySvg).attr("y", y);
    }),
  );

  // return svg.node()!.outerHTML;
  const dir = "/tmp/";
  fs.writeFileSync(dir + "dayview.svg", svg.node()!.outerHTML);

  // auto refresh html
  // const meta = document.createElement("meta");
  // meta.setAttribute("http-equiv", "refresh");
  // meta.setAttribute("content", "1");
  // document.head.append(meta);
  fs.writeFileSync(dir + "dayview.html", document.documentElement.outerHTML);
}

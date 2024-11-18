import fs from "fs";
import * as d3 from "d3";
import { DayviewCmdArgs } from "../commands/dayviewCmd";
import { connectDb } from "../auth/connectDb";
import { DateTime } from "luxon";
import { domdoc } from "./domdoc";
import { singleDay } from "./singleday";
import { parseDateStr } from "../time/parseDateStr";

export async function dayview(args: DayviewCmdArgs): Promise<void> {
  const db = connectDb(args);
  const endDate: DateTime<true> = args.endDate
    ? parseDateStr({ dateStr: args.endDate })
    : DateTime.local();

  let nDays = args.nDays;
  if (args.startDate !== undefined) {
    const startDate = parseDateStr({ dateStr: args.startDate });
    const diffDays = endDate.diff(startDate, "days").days + 1;
    if (nDays === undefined) {
      nDays = diffDays;
    } else if (nDays !== diffDays) {
      throw new Error(
        `nDays ${nDays} does not match the difference between startDate and endDate`,
      );
    }
  } else if (nDays === undefined) {
    nDays = 1;
  }

  const width = args.width ?? 2000;
  const margin = 10;
  const timeAxisHeight = 20;

  const plotWidth = width - 2 * margin;

  let height: number,
    dayHeight: number,
    dataHeight: number,
    plotHeight: number,
    interdayMargin: number;
  if (args.height === undefined && args.dayHeight === undefined) {
    dayHeight = 100;
    interdayMargin = 15;
    dataHeight = nDays * (dayHeight + interdayMargin) - interdayMargin;
    plotHeight = dataHeight + timeAxisHeight;
    height = plotHeight + 2 * margin;
  } else if (args.height !== undefined && args.dayHeight === undefined) {
    height = args.height;
    plotHeight = height - 2 * margin;
    dataHeight = plotHeight - timeAxisHeight;
    interdayMargin = 15;
    dayHeight = (dataHeight + interdayMargin) / nDays - interdayMargin;
  } else if (args.height === undefined && args.dayHeight !== undefined) {
    dayHeight = args.dayHeight;
    interdayMargin = 15;
    dataHeight = nDays * dayHeight + (nDays - 1) * interdayMargin;
    plotHeight = dataHeight + timeAxisHeight;
    height = plotHeight + 2 * margin;
  } else {
    height = args.height!;
    dayHeight = args.dayHeight!;
    plotHeight = height - 2 * margin;
    dataHeight = plotHeight - timeAxisHeight;
    interdayMargin = (dataHeight - dayHeight * nDays) / (nDays - 1);
    if (interdayMargin < 0) {
      throw new Error(
        "days are overlapping, set larget height or smaller day height",
      );
    }
  }

  const document = domdoc();

  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("width", width)
    .attr("height", height);

  const warning_icon = fs.readFileSync(
    __dirname + "/symbols/warning_sign.svg",
    "utf8",
  );
  svg
    .append("defs")
    .append("symbol")
    .attr("id", "warning-icon")
    .html(() => warning_icon);

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

  const days = Array.from({ length: nDays }, (_, i) => {
    return endDate.minus({ days: nDays - 1 - i }).toISODate();
  });
  const dayLabelFmt = "ccc\nLLL dd\nyyyy";

  const dateAxis = plot.append("g");
  days.forEach((date, i) => {
    const y = i * (dayHeight + interdayMargin);
    const dayLabels = DateTime.fromISO(date).toFormat(dayLabelFmt).split("\n");
    const nLabels = dayLabels.length;
    const g = dateAxis.append("g").attr("transform", `translate(0, ${y})`);

    dayLabels.forEach((dayLabel, j) => {
      g.append("text")
        .attr("x", "-0.5em")
        .attr("y", ((j + 0.5) * dayHeight) / nLabels)
        .attr("dy", "0.35em")
        .attr("fill", "white")
        .attr("text-anchor", "end")
        .text(dayLabel);
    });
  });
  // TODO: once actually rendering with a frontend make this dynamic
  const labelWidth = 70;

  dateAxis.attr("transform", `translate(${labelWidth}, 0)`);
  const dataWidth = plotWidth - labelWidth;

  const timeScale = d3
    .scaleUtc()
    .domain([new Date("2024-10-31"), new Date("2024-11-01")]) // 🎃
    .range([0, dataWidth]);

  const timeAxis = d3.axisBottom(timeScale).ticks(d3.timeHour.every(1), "%H");

  const axis = plot
    .append("g")
    .attr(
      "transform",
      `translate(${labelWidth}, ${plotHeight - timeAxisHeight})`,
    );
  axis.call(timeAxis).selectAll("path").attr("stroke", "white");
  axis.call(timeAxis).selectAll("line").attr("stroke", "white");
  axis.call(timeAxis).selectAll("text").attr("stroke", "white");

  const dataArea = plot
    .append("svg")
    .attr("class", "dataArea")
    .attr("width", dataWidth)
    .attr("height", dataHeight)
    .attr("x", labelWidth);

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

  const allErrors = dataArea.selectAll(".error");
  if (allErrors.size() > 0) {
    const _errorIcon = svg
      .append("use")
      .attr("href", "#warning-icon")
      .attr("x", 0)
      .attr("y", 0)
      .attr("width", 20)
      .attr("height", 20);

    const erroredFields = new Set<string>();
    allErrors.each(function () {
      const field = d3.select(this).attr("field");
      erroredFields.add(field);
    });
    const _errorText = svg
      .append("text")
      .attr("x", 25)
      .attr("y", 10)
      .attr("dy", "0.35em")
      .attr("fill", "red")
      .attr("text-anchor", "start")
      .text(Array.from(erroredFields).join(", "));
  }

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

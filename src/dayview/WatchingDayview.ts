import fs from "fs";
import * as d3 from "d3";
import { DateTime } from "luxon";
import { DayviewCmdArgs } from "../commands/dayviewCmd";
import { connectDb } from "../auth/connectDb";
import { domdoc } from "./domdoc";
import { singleDay } from "./singleday";
import { parseDateStr } from "../time/parseDateStr";
import { warningIcon } from "./symbols/warningIcon";
import xmlFormatter from "xml-formatter";
import {
  EitherPayload,
  isOccurredData,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import { globalPatternCache } from "./patternCache";

export class WatchingDayview {
  private db: PouchDB.Database;
  private args: DayviewCmdArgs;
  private document: Document;
  private svg: d3.Selection<SVGSVGElement, unknown, HTMLElement, any>;
  private dataArea: d3.Selection<SVGSVGElement, unknown, HTMLElement, any>;
  private days: string[];
  private dayElements: Map<string, SVGElement> = new Map();
  private dayHeight: number;
  private interdayMargin: number;
  private dataWidth: number;
  private width: number;
  private height: number;
  private labelWidth: number;
  private isInitialized = false;
  private cachedDays: string[] = [];
  private errorDisplayElement: d3.Selection<
    SVGGElement,
    unknown,
    HTMLElement,
    any
  > | null = null;

  constructor(args: DayviewCmdArgs) {
    this.args = args;
    this.db = connectDb(args);
    this.document = domdoc("dayview");
    this.svg = d3.select(this.document.body).append("svg") as any;
    this.dataArea = null as any;
    globalPatternCache.setDefs(this.svg.append("defs"));
    this.days = [];
    this.dayHeight = 0;
    this.interdayMargin = 0;
    this.dataWidth = 0;
    this.width = 0;
    this.height = 0;
    this.labelWidth = 0;
  }

  private setupDimensions() {
    const endDate: DateTime<true> = this.args.endDate
      ? parseDateStr({ dateStr: this.args.endDate })
      : DateTime.local();

    let nDays = this.args.nDays;
    if (this.args.startDate !== undefined) {
      const startDate = parseDateStr({ dateStr: this.args.startDate });
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

    this.width = this.args.width ?? 2000;
    const margin = 2;
    const timeAxisHeight = this.args.timeAxisHeight ?? 15;
    const plotWidth = this.width - 2 * margin;

    let plotHeight: number, dataHeight: number;
    if (this.args.height === undefined && this.args.dayHeight === undefined) {
      this.dayHeight = 100;
      this.interdayMargin = 15;
      dataHeight =
        nDays * (this.dayHeight + this.interdayMargin) - this.interdayMargin;
      plotHeight = dataHeight + timeAxisHeight;
      this.height = plotHeight + 2 * margin;
    } else if (
      this.args.height !== undefined &&
      this.args.dayHeight === undefined
    ) {
      this.height = this.args.height;
      plotHeight = this.height - 2 * margin;
      dataHeight = plotHeight - timeAxisHeight;
      this.interdayMargin = 15;
      this.dayHeight =
        (dataHeight + this.interdayMargin) / nDays - this.interdayMargin;
    } else if (
      this.args.height === undefined &&
      this.args.dayHeight !== undefined
    ) {
      this.dayHeight = this.args.dayHeight;
      this.interdayMargin = 15;
      dataHeight = nDays * this.dayHeight + (nDays - 1) * this.interdayMargin;
      plotHeight = dataHeight + timeAxisHeight;
      this.height = plotHeight + 2 * margin;
    } else {
      this.height = this.args.height!;
      this.dayHeight = this.args.dayHeight!;
      plotHeight = this.height - 2 * margin;
      dataHeight = plotHeight - timeAxisHeight;
      this.interdayMargin = (dataHeight - this.dayHeight * nDays) / (nDays - 1);
      if (this.interdayMargin < 0) {
        throw new Error(
          "days are overlapping, set larger height or smaller day height",
        );
      }
    }

    const newDays = Array.from({ length: nDays }, (_, i) => {
      return endDate.minus({ days: nDays - 1 - i }).toISODate();
    });

    if (JSON.stringify(this.cachedDays) !== JSON.stringify(newDays)) {
      this.days = newDays;
      this.cachedDays = [...newDays];
    } else {
      this.days = this.cachedDays;
    }

    const dayLabelFmt = "ccc\nLLL dd\nyyyy";
    const nLabels = dayLabelFmt.split("\n").length;
    const fontSize = Math.min(this.dayHeight / nLabels - 6, 20);
    this.labelWidth = fontSize * 4;
    this.dataWidth = plotWidth - this.labelWidth;

    return {
      margin,
      plotWidth,
      plotHeight,
      dataHeight,
      timeAxisHeight,
      fontSize,
      dayLabelFmt,
    };
  }

  async initialize() {
    const {
      margin,
      plotWidth,
      plotHeight,
      dataHeight,
      timeAxisHeight,
      dayLabelFmt,
    } = this.setupDimensions();

    this.svg
      .attr("xmlns", "http://www.w3.org/2000/svg")
      .attr("xmlns:xlink", "http://www.w3.org/1999/xlink")
      .attr("width", this.width)
      .attr("height", this.height);

    const defs = this.svg.append("defs");
    defs
      .append("symbol")
      .attr("id", "warning-icon")
      .html(() => warningIcon);
    defs.append("style").text(`svg { overflow: visible; }`);

    this.svg
      .append("rect")
      .attr("width", "100%")
      .attr("height", "100%")
      .attr("fill", "black");

    const plot = this.svg
      .append("svg")
      .attr("class", "plot")
      .attr("x", margin)
      .attr("y", margin)
      .attr("width", plotWidth)
      .attr("height", plotHeight);

    const fontSize = Math.min(this.dayHeight / 3 - 6, 20);
    const dateAxis = plot.append("g");
    this.days.forEach((date, i) => {
      const y = i * (this.dayHeight + this.interdayMargin);
      const dayLabels = DateTime.fromISO(date)
        .toFormat(dayLabelFmt)
        .split("\n");
      const g = dateAxis.append("g").attr("transform", `translate(0, ${y})`);

      dayLabels.forEach((dayLabel, j) => {
        g.append("text")
          .attr("x", "-0.5em")
          .attr("y", ((j + 0.5) * this.dayHeight) / dayLabels.length)
          .attr("dy", "0.35em")
          .attr("fill", "white")
          .attr("text-anchor", "end")
          .attr("font-size", fontSize)
          .text(dayLabel);
      });
    });

    dateAxis.attr("transform", `translate(${this.labelWidth}, 0)`);

    const timeScale = d3
      .scaleUtc()
      .domain([new Date("2024-10-31"), new Date("2024-11-01")])
      .range([0, this.dataWidth]);

    this.dataArea = plot
      .append("svg")
      .attr("class", "dataArea")
      .attr("width", this.dataWidth)
      .attr("height", dataHeight)
      .attr("x", this.labelWidth);

    const timeAxis = d3.axisBottom(timeScale).ticks(d3.timeHour.every(1), "%H");

    const axis = plot
      .append("g")
      .attr(
        "transform",
        `translate(${this.labelWidth}, ${plotHeight - timeAxisHeight})`,
      )
      .call(timeAxis);
    axis.selectAll("path").attr("stroke", "white");
    axis.selectAll("line").attr("stroke", "white");
    if (timeAxisHeight <= 6) {
      axis.selectAll("text").remove();
    } else {
      axis
        .selectAll("text")
        .attr("stroke", "white")
        .attr("fill", "white")
        .style("font-size", `${timeAxisHeight - 6}px`);
    }

    plot
      .append("g")
      .attr("transform", `translate(${this.labelWidth}, 0)`)
      .selectAll(".gridline")
      .data(timeScale.ticks(d3.utcHour.every(1)!))
      .enter()
      .append("line")
      .attr("class", "gridline")
      .attr("x1", (d) => timeScale(d))
      .attr("x2", (d) => timeScale(d))
      .attr("y1", 0)
      .attr("y2", plotHeight - timeAxisHeight)
      .attr("stroke", "white")
      .attr("stroke-opacity", (d) => (d.getUTCHours() % 3 === 0 ? 0.4 : 0.1));

    await this.renderAllDays();
    this.isInitialized = true;
  }

  private async renderAllDays() {
    const dayResults = await Promise.all(
      this.days.map(async (date, i) => {
        const y = i * (this.dayHeight + this.interdayMargin);
        const daySvg = await singleDay({
          db: this.db,
          date,
          dataWidth: this.dataWidth,
          height: this.dayHeight,
          labelWidth: 0,
        });
        return { date, svg: daySvg, y };
      }),
    );

    dayResults.forEach(({ date, svg, y }) => {
      const dayElement = this.dataArea.append(() => svg).attr("y", y);
      this.dayElements.set(date, dayElement.node() as SVGElement);
    });

    this.updateErrorDisplay();
  }

  async updateDay(date: string) {
    if (!this.isInitialized || !this.days.includes(date)) {
      return;
    }

    const dayIndex = this.days.indexOf(date);
    const y = dayIndex * (this.dayHeight + this.interdayMargin);

    const existingElement = this.dayElements.get(date);
    if (existingElement) {
      existingElement.remove();
    }

    const daySvg = await singleDay({
      db: this.db,
      date,
      dataWidth: this.dataWidth,
      height: this.dayHeight,
      labelWidth: 0,
    });

    const newElement = this.dataArea.append(() => daySvg).attr("y", y);
    this.dayElements.set(date, newElement.node() as SVGElement);

    this.updateErrorDisplay();
  }

  private updateErrorDisplay() {
    this.svg.selectAll(".error-display").remove();

    const allErrors = this.dataArea.selectAll(".error");
    if (allErrors.size() > 0) {
      const erroredFields = new Set<string>();
      allErrors.each(function () {
        const field = d3.select(this).attr("field");
        erroredFields.add(field);
      });

      const errorGroup = this.svg.append("g").attr("class", "error-display");

      errorGroup
        .append("use")
        .attr("href", "#warning-icon")
        .attr("x", 0)
        .attr("y", this.height - 20)
        .attr("width", 20)
        .attr("height", 20);

      const errorText = Array.from(erroredFields).join(", ");
      errorGroup
        .append("rect")
        .attr("x", 25)
        .attr("y", this.height - 20)
        .attr("width", errorText.length * 8)
        .attr("height", 20)
        .attr("fill", "red");

      errorGroup
        .append("text")
        .attr("x", 25)
        .attr("y", this.height - 10)
        .attr("dy", "0.35em")
        .attr("fill", "black")
        .attr("text-anchor", "start")
        .text(errorText);
    }
  }

  getOutput(): string {
    const prettySvg = xmlFormatter(this.svg.node()!.outerHTML);
    const outputFile = this.args.outputFile;

    if (outputFile === undefined) {
      return prettySvg;
    }
    if (outputFile.endsWith(".svg")) {
      const xmlDeclaration =
        '<?xml version="1.0" encoding="UTF-8" standalone="no"?>\n';
      fs.writeFileSync(outputFile, xmlDeclaration + prettySvg);
      return prettySvg;
    } else if (outputFile.endsWith(".html")) {
      const prettyHtml = xmlFormatter(this.document.documentElement.outerHTML);
      fs.writeFileSync(outputFile, prettyHtml);
      return prettySvg;
    } else {
      throw new Error("output file must have a .html or .svg extension");
    }
  }

  determineDayFromChange(
    change: PouchDB.Core.ChangesResponseChange<EitherPayload>,
  ): string | null {
    if (!change.doc) {
      return null;
    }

    // For DatumPayload, data is in doc.data; for DataOnlyPayload, data is at root level
    const dataToCheck = isDatumPayload(change.doc)
      ? change.doc.data
      : change.doc;

    // Check for occurTime first
    if (isOccurredData(dataToCheck)) {
      const occurTime = DateTime.fromISO(dataToCheck.occurTime.utc);
      const dayISO = occurTime.toISODate();
      if (dayISO && this.days.includes(dayISO)) {
        return dayISO;
      }
    }

    // Fall back to createTime if no occurTime
    if (isDatumPayload(change.doc) && change.doc.meta?.createTime) {
      const createTime = DateTime.fromISO(change.doc.meta.createTime.utc);
      const dayISO = createTime.toISODate();
      if (dayISO && this.days.includes(dayISO)) {
        return dayISO;
      }
    }

    return null;
  }
}

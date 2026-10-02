import * as d3 from "d3";
import { DatumState } from "../state/normalizeState";
import { patternId } from "../utils/patternId";
import { getStateColor } from "../field/fieldColor";

export type PatternConfig = {
  states: DatumState[];
  patternId: string;
};

export class PatternCache {
  private patterns = new Map<string, PatternConfig>();
  private patternElements = new Map<
    string,
    d3.Selection<SVGPatternElement, unknown, HTMLElement, any>
  >();
  private defs: d3.Selection<SVGDefsElement, unknown, HTMLElement, any> | null =
    null;

  constructor(svg?: d3.Selection<SVGSVGElement, unknown, HTMLElement, any>) {
    if (svg) {
      this.initializeDefs(svg);
    }
  }

  private initializeDefs(
    svg: d3.Selection<SVGSVGElement, unknown, HTMLElement, any>,
  ) {
    this.defs = svg.select("defs");
    if (this.defs.empty()) {
      this.defs = svg.append("defs");
    }
  }

  public initializeDefsSvg(svg: any) {
    this.defs = svg.select("defs");
    if (this.defs && this.defs.empty()) {
      this.defs = svg.append("defs");
    }
  }

  setDefs(defs: d3.Selection<SVGDefsElement, unknown, HTMLElement, any>) {
    this.defs = defs;
  }

  private generatePatternKey(states: DatumState[]): string {
    return states
      .map((s) => s?.toString() || "null")
      .sort()
      .join("|");
  }

  getOrCreatePattern(states: DatumState[], field?: string): string {
    const key = this.generatePatternKey(states) + (field ? `_${field}` : "");

    if (this.patterns.has(key)) {
      return this.patterns.get(key)!.patternId;
    }

    if (!this.defs) {
      throw new Error("Pattern cache not initialized with defs element");
    }

    const id = patternId(states) + (field ? `_${field}` : "");
    const stripeWidth = 8;
    const height = 20;

    const pattern = this.defs
      .append("pattern")
      .attr("id", id)
      .attr("patternUnits", "userSpaceOnUse")
      .attr("width", stripeWidth * states.length)
      .attr("height", height)
      .attr("patternTransform", "rotate(45)");

    states.forEach((state, index) => {
      const color = field
        ? getStateColor({ state, field })
        : state
          ? "#4CAF50"
          : "#757575";
      pattern
        .append("rect")
        .attr("x", index * stripeWidth)
        .attr("y", -height)
        .attr("width", stripeWidth)
        .attr("height", height * 3)
        .attr("fill", color);
    });

    const config: PatternConfig = {
      states: [...states],
      patternId: id,
    };

    this.patterns.set(key, config);
    this.patternElements.set(id, pattern);

    return id;
  }

  getPattern(states: DatumState[]): string | null {
    const key = this.generatePatternKey(states);
    return this.patterns.get(key)?.patternId || null;
  }

  clearCache() {
    this.patterns.clear();
    this.patternElements.clear();
    if (this.defs) {
      this.defs.selectAll("pattern").remove();
    }
  }

  getStats() {
    return {
      cachedPatterns: this.patterns.size,
      patternElements: this.patternElements.size,
    };
  }
}

export const globalPatternCache = new PatternCache();

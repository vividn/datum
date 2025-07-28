import * as d3 from "d3";
import { occurredFields } from "../field/occurredFields";
import { fieldSvgBlocks } from "./fieldSvgBlocks";
import { domdoc } from "./domdoc";
import { getFieldSpec } from "../field/mySpecs";
import md5 from "md5";
import { globalPatternCache } from "./patternCache";
export type AllFieldsSvgType = {
  db: PouchDB.Database;
  startUtc: string;
  endUtc: string;
  width: number;
  height: number;
};

const fieldSortCache = new Map<
  string,
  Array<{ field: string; y1: number; fieldHeight: number; zIndex: number }>
>();

function getCachedFieldSort(
  fields: string[],
  height: number,
): Array<{ field: string; y1: number; fieldHeight: number; zIndex: number }> {
  const cacheKey = `${fields.slice().sort().join(",")}_${height}`;

  if (fieldSortCache.has(cacheKey)) {
    return fieldSortCache.get(cacheKey)!;
  }

  const sortedWithSpans = fields
    .map((field) => {
      const spec = getFieldSpec(field);
      const specY =
        spec.y ?? parseInt(md5(field).slice(0, 8), 16) / Math.pow(2, 32);
      const defaultHeight = 0.12;

      let pY: number, pHeight: number;
      if (spec.height !== undefined) {
        pY = specY;
        pHeight = spec.height;
      } else {
        pY = specY - defaultHeight / 2;
        pHeight = defaultHeight;
      }

      const y1 = pY * height;
      const zIndex = spec.zIndex ?? pY;
      const fieldHeight = Math.ceil(pHeight * height);
      return { field, y1, fieldHeight, zIndex };
    })
    .sort((a, b) => a.zIndex - b.zIndex);

  fieldSortCache.set(cacheKey, sortedWithSpans);

  if (fieldSortCache.size > 5) {
    const firstKey = fieldSortCache.keys().next().value;
    if (firstKey) {
      fieldSortCache.delete(firstKey);
    }
  }

  return sortedWithSpans;
}

export async function allFieldsSvg(args: AllFieldsSvgType) {
  const { db, startUtc, endUtc, width, height } = args;
  const document = domdoc();
  const svg = d3.select(document.body).append("svg").attr("class", `timechunk`);

  globalPatternCache.initializeDefsSvg(svg);

  const allFields = await occurredFields(db);
  const sortedWithSpans = getCachedFieldSort(allFields, height);

  const fieldSvgs = await Promise.all(
    sortedWithSpans.map(async (fieldSpan) => {
      return fieldSvgBlocks({
        db,
        field: fieldSpan.field,
        startUtc: startUtc!,
        endUtc: endUtc!,
        width,
        height: fieldSpan.fieldHeight,
      });
    }),
  );
  fieldSvgs.forEach((fieldSvg: any, i: number) => {
    if (fieldSvg === null) {
      return;
    }
    const fieldSpan = sortedWithSpans[i];
    svg
      .append(() => fieldSvg)
      .attr("x", 0)
      .attr("y", fieldSpan.y1)
      .attr("width", width)
      .attr("height", fieldSpan.fieldHeight);
  });

  return svg.node();
}

export async function allFieldsSvgOptimized(args: AllFieldsSvgType) {
  return allFieldsSvg(args);
}

import * as d3 from "d3";
import { JSDOM } from "jsdom";
import { occurredFields } from "../field/occurredFields";
import { getSpan } from "./getSpan";
import { fieldSvgBlocks } from "./fieldSvgBlocks";
export type AllFieldsSvgType = {
  db: PouchDB.Database;
  startUtc: string;
  endUtc: string;
  width: number;
  height: number;
};

export async function allFieldsSvg(args: AllFieldsSvgType) {
  const { db, startUtc, endUtc, width, height } = args;
  const document = new JSDOM().window.document;
  const svg = d3.select(document.body).append("svg").attr("class", `timechunk`);

  const allFields = await occurredFields(db);
  const sortedWithSpans = allFields
    .map((field) => {
      const [p1, pHeight] = getSpan(field);
      const y1 = p1 * height;
      const fieldHeight = pHeight * height;
      return { field, y1, fieldHeight };
    })
    .sort((a, b) => a.y1 - b.y1);

  await Promise.all(
    sortedWithSpans.map(async (fieldSpan) => {
      const fieldSvg = await fieldSvgBlocks({
        db,
        field: fieldSpan.field,
        startUtc,
        endUtc,
        width,
        height: fieldSpan.fieldHeight,
      });
      if (fieldSvg === null) {
        return null;
      }

      return svg
        .append(() => fieldSvg)
        .attr("x", 0)
        .attr("y", fieldSpan.y1)
        .attr("width", width)
        .attr("height", fieldSpan.fieldHeight);
    }),
  );

  return svg.node();
}

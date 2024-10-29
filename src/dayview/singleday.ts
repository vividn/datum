import { DateTime } from "luxon";
import { occurredFields } from "../field/occurredFields";
import { getSpan } from "./getSpan";
import { fieldSvgBlocks } from "./fieldSvgBlocks";

export type SingleDayArgs = {
  db: PouchDB.Database;
  date: string;
  width: number;
  height: number;
};

export async function singleDay(args: SingleDayArgs) {
  const { db, date, width, height } = args;

  // For now just use local time
  // TODO: get timezone stats and handle timezone changes

  const day: DateTime<true> = DateTime.fromISO(date) as DateTime<true>;
  const startUtc = day.startOf("day").toUTC().toISO();
  const endUtc = day.endOf("day").toUTC().toISO();

  const allFields = await occurredFields(db);
  const sortedWithSpans = allFields
    .map((field) => {
      const [p1, pHeight] = getSpan(field);
      const y1 = p1 * height;
      const fieldHeight = pHeight * height;
      return { field, y1, fieldHeight };
    })
    .sort((a, b) => a.y1 - b.y1);

  const fieldSvgs = await Promise.all(
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

    }),
  );
  )
  const sortableGroups = await Promise.all(
    allFields.map(async (field) => {
      const [p1, pHeight] = getSpan(field);
      const y1 = p1 * width;
      const fieldHeight = height * pHeight;
      const fieldSvg = await fieldSvgBlocks({
        db,
        field,
        startUtc,
        endUtc,
        width,
        height,
      });
      return { field, y1, fieldHeight, svg: fieldSvg };
    }),
  );
}

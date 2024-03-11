import Table from "easy-table";
import { stringify } from "querystring";
import { EitherDocument } from "../documentControl/DatumDocument";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import { TIME_METRICS } from "../views/datumViews/timingView";
import { extractFormatted } from "./output";

type TableOutputArgs = OutputArgs & {
  columns?: string[];
  timeMetric?: (typeof TIME_METRICS)[number] | "none";
};

export function tableOutput(
  docs: EitherDocument[],
  args: TableOutputArgs,
): string | undefined {
  const format = args.formatString;
  const show = args.show;
  const metric = args.timeMetric ?? "hybrid";
  const columns = args.columns || [];
  if (show === Show.None) {
    return undefined;
  }
  if (format) {
    docs.forEach((doc) => {
      const { data, meta } = pullOutData(doc);
      console.log(
        interpolateFields({ data, meta, format, useHumanTimes: true }),
      );
    });
  }
  const formattedRows: Record<string, string | undefined>[] = docs.map(
    (doc) => {
      const formatted = extractFormatted(doc);
      const baseColumns = {
        time: formatted.time?.[metric],
        field: formatted.field,
        state: formatted.state,
        duration: formatted.dur,
        hid: formatted.hid,
      };
      if (columns.length === 0) {
        return baseColumns;
      }

      const { data } = pullOutData(doc);
      const extraColumns: Record<string, string> = {};
      columns.forEach((col) => {
        const columnValue = data[col];
        if (columnValue !== undefined) {
          extraColumns[col] = stringify(columnValue);
        }
      });
      return { ...baseColumns, ...extraColumns };
    },
  );

  const headerRow: Record<string, string | undefined> = {
    time: formattedRows.some((row) => row.time !== undefined)
      ? "time"
      : undefined,
    duration: formattedRows.some((row) => row.duration !== undefined)
      ? "dur"
      : undefined,
    field: "field",
    state: formattedRows.some((row) => row.state !== undefined)
      ? "state"
      : undefined,
    hid: formattedRows.some((row) => row.hid !== undefined) ? "hid" : undefined,
  };
  columns.forEach((col) => {
    if (formattedRows.some((row) => row[col] !== undefined)) {
      headerRow[col] = col;
    }
  });

  const allRows = [headerRow, ...formattedRows];

  return Table.print(allRows, { time: { printer: Table.padLeft } }, (table) => {
    return table.print();
  });
}

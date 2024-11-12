import Table from "easy-table";
import { EitherDocument } from "../documentControl/DatumDocument";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import { TIME_METRICS } from "../views/datumViews/timingView";
import { extractFormatted } from "./output";
import stringWidth from "string-width";

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
  if (docs.length === 0 && show !== Show.Format) {
    return "[No data]";
  }

  if (format) {
    const formattedRows = docs.map((doc) => {
      const { data, meta } = pullOutData(doc);
      return interpolateFields({ data, meta, format, useHumanTimes: true });
    });
    return formattedRows.join("\n");
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
          extraColumns[col] = JSON.stringify(columnValue);
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
    field: formattedRows.some((row) => row.field === undefined)
      ? undefined
      : formattedRows.some((row) => stringWidth(row.field ?? "") >= 5)
        ? "field"
        : "f",
    state: formattedRows.some((row) => row.state === undefined)
      ? undefined
      : formattedRows.some((row) => stringWidth(row.state ?? "") >= 5)
        ? "state"
        : "s",
    hid: formattedRows.some((row) => row.hid !== undefined) ? "hid" : undefined,
  };
  columns.forEach((col) => {
    headerRow[col] = col;
  });

  const allRows = [headerRow, ...formattedRows];

  return Table.print(allRows, { time: { printer: Table.padLeft } }, (table) => {
    return table.print();
  });
}

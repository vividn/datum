import Table from "easy-table";
import { EitherDocument } from "../documentControl/DatumDocument";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import { TIME_METRICS } from "../views/datumViews/timingView";
import { extractFormatted } from "./output";
import stringWidth from "string-width";
import { OutputFunction } from "./outputUtils";

type TableOutputArgs = OutputArgs & {
  columns?: string[];
  timeMetric?: (typeof TIME_METRICS)[number] | "none";
};

// Default output function
const defaultOutput: OutputFunction = (message: string) => {
  console.log(message);
};

export interface TableOutputResult {
  output: string | undefined;
  rows?: string[];
  formattedTable?: string;
}

export function tableOutput(
  docs: EitherDocument[],
  args: TableOutputArgs,
  output: OutputFunction = defaultOutput,
): TableOutputResult {
  const format = args.formatString;
  const show = args.show;
  const metric = args.timeMetric ?? "hybrid";
  const columns = args.columns || [];

  if (show === Show.None) {
    return { output: undefined };
  }

  if (docs.length === 0 && show !== Show.Format) {
    const noDataMessage = "[No data]";
    output(noDataMessage);
    return { output: noDataMessage };
  }

  if (format) {
    const formattedRows = docs.map((doc) => {
      const { data, meta } = pullOutData(doc);
      return interpolateFields({ data, meta, format, useHumanTimes: true });
    });
    const formattedOutput = formattedRows.join("\n");
    output(formattedOutput);
    return { output: formattedOutput, rows: formattedRows };
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

  const formattedTable = Table.print(
    allRows,
    { time: { printer: Table.padLeft } },
    (table) => {
      return table.print();
    },
  );

  if (formattedTable) {
    output(formattedTable);
  }

  return {
    output: formattedTable,
    formattedTable,
    rows: formattedRows.map((row) => JSON.stringify(row)),
  };
}

import Table from "easy-table";
import { EitherDocument } from "../documentControl/DatumDocument";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import { extractFormatted } from "./output";

export function tableOutput(docs: EitherDocument[], args: OutputArgs): string {
  const format = args.formatString;
  const show = args.show;
  if (format && show !== Show.None) {
    docs.forEach((doc) => {
      const { data, meta } = pullOutData(doc);
      console.log(
        interpolateFields({ data, meta, format, useHumanTimes: true }),
      );
    });
  }
  if (format === undefined && show !== Show.None) {
    const formattedRows = docs.map((doc) => {
      const formatted = extractFormatted(doc);
      return {
        time: formatted.time?.[metric],
        field: formatted.field,
        state: formatted.state,
        duration: formatted.dur,
        hid: formatted.hid,
      };
    });
    const headerRow = {
      time: "Time",
      duration: formattedRows.some((row) => row.duration !== undefined)
        ? "Dur"
        : undefined,
      field: "Field",
      state: formattedRows.some((row) => row.state !== undefined)
        ? "State"
        : undefined,
      hid: "HID",
    };

    const allRows = [headerRow, ...formattedRows];
    console.log(
      Table.print(allRows, { time: { printer: Table.padLeft } }, (table) => {
        return table.print();
      }),
    );
  }
}

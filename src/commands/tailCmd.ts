import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { viewMap } from "../views/viewMap";
import { connectDb } from "../auth/connectDb";
import { occurTimeView } from "../views/datumViews";
import { interpolateFields } from "../utils/interpolateFields";
import { MainDatumArgs } from "../input/mainYargs";
import { fieldArgs } from "../input/fieldArgs";
import { pullOutData } from "../utils/pullOutData";
import { extractFormatted } from "../output/output";
import Table from "easy-table";

export const command = ["tail [field]"];
export const desc =
  "show the most recently occured/modified/created entries in the db";

export type TailCmdArgs = MainDatumArgs & {
  num?: number;
  field?: string;
  metric?: "occur" | "create" | "modify";
  format?: string;
};

export function builder(yargs: Argv): Argv {
  return fieldArgs(yargs).options({
    num: {
      alias: ["n", "number"],
      describe: "number of entries to show, defaults to 10",
      type: "number",
    },
    // TODO
    // date: {
    //   describe:
    //     "Show all that happened on a date instead of the most recent. Unless -n is specified, will return all",
    //   nargs: 1,
    //   type: "string",
    // },
    // metric: {
    //   describe: "which time to use for the sorting, default is hybrid: occur or modify",
    //   choices: ["hybrid", "occur", "create", "modify"],
    //   alias: "m",
    //   type: "string",
    // },
    // head: {
    //   describe: "show first rows instead of last rows",
    //   type: "boolean",
    // },
    // view: {
    //   describe: "specify a specific view to use instead of the built in time views",
    //   nargs: 1,
    //   type: "string"
    // }
  });
}

export async function tailCmd(args: TailCmdArgs): Promise<EitherDocument[]> {
  const db = connectDb(args);

  const limit = args.num ?? 10;
  const viewResults = await viewMap({
    db,
    datumView: occurTimeView,
    params: {
      descending: true,
      startkey: "\uffff\uffff",
      limit,
      include_docs: true,
    },
  });
  const rawRows = viewResults.rows.reverse();
  const docs: EitherDocument[] = rawRows.map((row) => row.doc!);
  const format = args.formatString;
  if (format) {
    docs.forEach((doc) => {
      const { data, meta } = pullOutData(doc);
      console.log(
        interpolateFields({ data, meta, format, useHumanTimes: true })
      );
    });
    return docs;
  }

  const formattedRows = docs.map((doc) => {
    const formatted = extractFormatted(doc);
    return {
      time: formatted.occurTimeText,
      field: formatted.fieldText,
      state: formatted.stateText,
      duration: formatted.durText,
      hid: formatted.hidText,
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
  // console.log(Table.print(formattedRows, { time: { printer: Table.padLeft } }));
  console.log(
    Table.print(allRows, { time: { printer: Table.padLeft } }, (table) => {
      return table.print();
    })
  );

  return docs;
}

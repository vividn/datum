import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { viewMap } from "../views/viewMap";
import { connectDb } from "../auth/connectDb";
import { interpolateFields } from "../utils/interpolateFields";
import { MainDatumArgs } from "../input/mainYargs";
import { FieldArgs, fieldArgs } from "../input/fieldArgs";
import { pullOutData } from "../utils/pullOutData";
import { extractFormatted } from "../output/output";
import Table from "easy-table";
import { TIME_METRICS, timingView } from "../views/datumViews/tail";
import { HIGH_STRING, startsWith } from "../utils/startsWith";
import { handleTimeArgs, TimeArgs, timeYargs } from "../input/timeArgs";
import { reverseViewParams } from "../utils/reverseViewParams";

export const command = ["tail [field]"];
export const desc =
  "show the most recently occured/modified/created entries in the db";

export type TailCmdArgs = MainDatumArgs &
  TimeArgs &
  FieldArgs & {
    n?: number;
    metric?: "occur" | "create" | "modify";
    head?: boolean;
  };

export function builder(yargs: Argv): Argv {
  return timeYargs(fieldArgs(yargs)).options({
    n: {
      alias: ["number"],
      describe: "number of entries to show, defaults to 10",
      type: "number",
    },
    metric: {
      describe:
        "which time to use for the sorting, default is hybrid: occur or modify",
      choices: TIME_METRICS,
      alias: "m",
      type: "string",
    },
    head: {
      describe: "show first rows instead of last rows",
      type: "boolean",
      hidden: true,
    },
  });
}

export async function tailCmd(args: TailCmdArgs): Promise<EitherDocument[]> {
  const db = connectDb(args);

  const limit = args.n ?? 10;
  const metric = args.metric ?? "hybrid";
  const field = args.field ?? null;

  let viewParams: PouchDB.Query.Options<any, any> = {
    include_docs: true,
    inclusive_end: true,
    limit,
  };

  const { timeStr, unmodified: isDefaultTime } = handleTimeArgs(args);
  if (isDefaultTime || timeStr === undefined) {
    viewParams.startkey = [metric, field, ""];
    viewParams.endkey = [metric, field, HIGH_STRING];
  } else if (!timeStr.includes("T")) {
    // when just a date is given, display all entries for that day unless limit is specifically given
    viewParams = {
      ...viewParams,
      ...startsWith([metric, field, timeStr]),
      limit: args.n,
    };
  } else {
    viewParams.startkey = [metric, field, ""];
    viewParams.endkey = [metric, field, timeStr];
  }

  if (args.head !== true) {
    viewParams = reverseViewParams(viewParams);
  }
  const viewResults = await viewMap({
    db,
    datumView: timingView,
    params: viewParams,
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
  // console.log(Table.print(formattedRows, { time: { printer: Table.padLeft } }));
  console.log(
    Table.print(allRows, { time: { printer: Table.padLeft } }, (table) => {
      return table.print();
    })
  );

  return docs;
}

import { EitherDocument } from "../documentControl/DatumDocument";
import { viewMap } from "../views/viewMap";
import { connectDb } from "../auth/connectDb";
import { interpolateFields } from "../utils/interpolateFields";
import { FieldArgs, fieldArgs } from "../input/fieldArgs";
import { pullOutData } from "../utils/pullOutData";
import { extractFormatted } from "../output/output";
import Table from "easy-table";
import {
  TIME_METRICS,
  timingView,
  TimingViewType,
} from "../views/datumViews/timingView";
import { HIGH_STRING } from "../utils/startsWith";
import { handleTimeArgs, timeArgs, TimeArgs } from "../input/timeArgs";
import { reverseViewParams } from "../utils/reverseViewParams";
import { outputArgs, Show } from "../input/outputArgs";
import { DateTime } from "luxon";
import { ArgumentParser, SUPPRESS } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { MainDatumArgs } from "../input/mainArgs";

export const tailArgs = new ArgumentParser({
  add_help: false,
  parents: [fieldArgs, timeArgs],
});
tailArgs.add_argument("-n", {
  help: "number of entries to show, defaults to 10",
  type: "int",
});
tailArgs.add_argument("--metric", "-m", {
  help: "which time to use for the sorting, default is hybrid: occur or modify",
  choices: [...TIME_METRICS],
});
tailArgs.add_argument("--watch", "-w", {
  help: "watch the db for changes and update",
  action: "store_true",
});
tailArgs.add_argument("--head", {
  help: "show first rows instead of last rows" || SUPPRESS,
  action: "store_true",
});

export const tailCmdArgs = new ArgumentParser({
  description:
    "show the most recently occurred/modified/created entries in the db",
  prog: "datum tail",
  usage: "%(prog)s [field]",
  parents: [tailArgs, dbArgs, outputArgs],
});

export type TailCmdArgs = MainDatumArgs &
  TimeArgs &
  FieldArgs & {
    n?: number;
    metric?: "hybrid" | "occur" | "create" | "modify";
    head?: boolean;
    watch?: boolean;
  };

export async function tailCmd(
  argsOrCli: TailCmdArgs | string | string[],
  preparsed?: Partial<TailCmdArgs>,
): Promise<EitherDocument[]> {
  const args = parseIfNeeded(tailCmdArgs, argsOrCli, preparsed);
  const db = connectDb(args);

  const limit = args.n ?? 10;
  const metric = args.metric ?? "hybrid";
  const field = args.field ?? null;

  let viewParams: PouchDB.Query.Options<any, any> = {
    include_docs: true,
    inclusive_end: true,
    limit,
  };

  const { time, unmodified: isDefaultTime, onlyDate } = handleTimeArgs(args);
  const utcTime = time?.utc;

  if (isDefaultTime || utcTime === undefined) {
    viewParams.startkey = [metric, field, ""];
    viewParams.endkey = [metric, field, HIGH_STRING];
  } else if (onlyDate) {
    // due to timezone shenanigans, must also grab the full days around the requested date and then filter later
    viewParams.limit = undefined;
    viewParams.startkey = [
      metric,
      field,
      DateTime.fromISO(utcTime)
        .minus({ day: 1 })
        .startOf("day")
        .toUTC()
        .toISO(),
    ];
    viewParams.endkey = [
      metric,
      field,
      DateTime.fromISO(utcTime).plus({ day: 1 }).endOf("day").toUTC().toISO(),
    ];
  } else if (args.head) {
    viewParams.startkey = [metric, field, utcTime];
    viewParams.endkey = [metric, field, HIGH_STRING];
  } else {
    viewParams.startkey = [metric, field, ""];
    viewParams.endkey = [metric, field, utcTime];
  }

  if (args.head !== true) {
    viewParams = reverseViewParams(viewParams);
  }
  const viewResults = await viewMap({
    db,
    datumView: timingView,
    params: viewParams,
  });

  // TODO factor this out better and automatically extract types from views
  const rawRows: {
    key: TimingViewType["MapKey"];
    value: TimingViewType["MapValue"];
    doc?: EitherDocument;
  }[] = args.head ? viewResults.rows : viewResults.rows.reverse();
  const filteredRows = onlyDate
    ? rawRows
        .filter((row) => row.value[0] === utcTime)
        // this sort moves times that are just dates to the top
        .sort((a, b) => (a.value >= b.value ? 1 : -1))
    : rawRows;
  const limitedRows =
    onlyDate && args.n === undefined
      ? filteredRows
      : args.head
        ? filteredRows.slice(0, limit)
        : filteredRows.slice(-limit);
  const docs: EitherDocument[] = limitedRows.map((row) => row.doc!);

  const format = args.formatString;
  const show = args.show;
  if (format && show !== Show.None) {
    docs.forEach((doc) => {
      const { data, meta } = pullOutData(doc);
      console.log(
        interpolateFields({ data, meta, format, useHumanTimes: true }),
      );
    });
    return docs;
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
  return docs;
}

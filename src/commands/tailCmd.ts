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
import {
  TIME_METRICS,
  timingView,
  TimingViewType,
} from "../views/datumViews/timingView";
import { HIGH_STRING } from "../utils/startsWith";
import { handleTimeArgs, TimeArgs, timeYargs } from "../input/timeArgs";
import { reverseViewParams } from "../utils/reverseViewParams";
import { Show } from "../input/outputArgs";
import { DateTime } from "luxon";
import { getTimezone } from "../time/getTimezone";

export const command = ["tail [field]", "head [field]"];
export const desc =
  "show the most recently occured/modified/created entries in the db";

export type TailCmdArgs = MainDatumArgs &
  TimeArgs &
  FieldArgs & {
    n?: number;
    metric?: "hybrid" | "occur" | "create" | "modify";
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

  const { timeStr, unmodified: isDefaultTime, onlyDate } = handleTimeArgs(args);

  if (isDefaultTime || timeStr === undefined) {
    viewParams.startkey = [metric, field, ""];
    viewParams.endkey = [metric, field, HIGH_STRING];
  } else if (onlyDate) {
    // when just a date is given, display all entries for that day unless limit is specifically given
    // due to timezone shenanigans, must also grab the full days around the requested date and then filter later
    viewParams.startkey = [
      metric,
      field,
      DateTime.fromISO(timeStr).minus({ day: 1 }).startOf("day"),
    ];
    viewParams.endkey = [
      metric,
      field,
      DateTime.fromISO(timeStr).plus({ day: 1 }).endOf("day"),
    ];
  } else if (args.head) {
    viewParams.startkey = [metric, field, timeStr];
    viewParams.endkey = [metric, field, HIGH_STRING];
  } else {
    viewParams.startkey = [metric, field, ""];
    viewParams.endkey = [metric, field, timeStr, Infinity];
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
    ? rawRows.filter((row) => {
        const dateOrTime = row.key[2];
        const utcOffset = row.key[3] ?? 0;
        const localDate = !dateOrTime.includes("T")
          ? dateOrTime
          : DateTime.fromISO(dateOrTime, {
              zone: getTimezone(utcOffset),
            }).toISODate();
        console.info({ localDate, timeStr });
        return localDate === timeStr;
      })
    : rawRows;
  console.info({ filteredRows });
  const docs: EitherDocument[] = filteredRows.map((row) => row.doc!);
  const format = args.formatString;
  const show = args.show;
  if (format && show !== Show.None) {
    docs.forEach((doc) => {
      const { data, meta } = pullOutData(doc);
      console.log(
        interpolateFields({ data, meta, format, useHumanTimes: true })
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
      })
    );
  }
  return docs;
}

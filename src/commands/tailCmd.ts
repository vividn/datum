import { EitherDocument } from "../documentControl/DatumDocument";
import { viewMap } from "../views/viewMap";
import { connectDb } from "../auth/connectDb";
import { FieldArgs, fieldArgs } from "../input/fieldArgs";
import {
  TIME_METRICS,
  timingView,
  TimingViewType,
} from "../views/datumViews/timingView";
import { HIGH_STRING } from "../utils/startsWith";
import { handleTimeArgs, timeArgs, TimeArgs } from "../input/timeArgs";
import { reverseViewParams } from "../utils/reverseViewParams";
import { outputArgs } from "../input/outputArgs";
import { DateTime } from "luxon";
import { ArgumentParser, SUPPRESS } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { MainDatumArgs } from "../input/mainArgs";
import { tableOutput } from "../output/tableOutput";
import { once } from "events";
import { QueryOptions } from "../utils/utilityTypes";

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
  help: SUPPRESS, // @dev: show first rows instead of last rows. Used by headCmd
  action: "store_true",
});
tailArgs.add_argument("--column", {
  help: "additional columns to show. Can be specified multiple times and/or take a comma separated list",
  action: "append",
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
    column?: string[];
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
  const columns = (args.column ?? []).map((col) => col.split(",")).flat();

  let viewParams: QueryOptions = {
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

  async function getAndDisplayTail(): Promise<EitherDocument<unknown>[]> {
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

    const output = tableOutput(docs, { ...args, timeMetric: metric, columns });
    if (output !== undefined) {
      if (args.watch) {
        console.clear();
      }
      console.log(output);
    }
    return docs;
  }

  if (args.watch) {
    let returnDocs: EitherDocument[];
    const changes = db.changes({
      since: "now",
      live: true,
    });
    changes.on("change", async () => {
      returnDocs = await getAndDisplayTail();
    });
    returnDocs = await getAndDisplayTail();
    // @ts-expect-error: PouchDB hasn't changed its even emitter to to match EventTarget yet
    await once(changes, "complete");
    return returnDocs;
  }
  return getAndDisplayTail();
}

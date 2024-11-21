import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { dayview } from "../dayview/dayview";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { connectDb } from "../auth/connectDb";

export const dayviewArgs = new ArgumentParser({
  add_help: false,
  parents: [],
});

dayviewArgs.add_argument("--n-days", "-n", {
  help: "number of days to show",
  type: "int",
  dest: "nDays",
});

dayviewArgs.add_argument("--start-date", {
  help: "start date to show",
  type: "str",
  dest: "startDate",
});

dayviewArgs.add_argument("--end-date", {
  help: "end date to show. Defaults to today",
  type: "str",
  dest: "endDate",
});

dayviewArgs.add_argument("--width", {
  help: "width of the svg",
  type: "int",
});

dayviewArgs.add_argument("--height", {
  help: "height of the svg",
  type: "int",
});

dayviewArgs.add_argument("--day-height", {
  help: "height of each day. If not specified, it will be calculated",
  type: "int",
  dest: "dayHeight",
});

dayviewArgs.add_argument("--output-file", "-o", {
  help: "output file, should have a .html or .svg extension",
  type: "str",
  dest: "outputFile",
});

dayviewArgs.add_argument("--watch", "-w", {
  help: "watch for changes",
  action: "store_true",
  dest: "watch",
});

export const dayviewCmdArgs = new ArgumentParser({
  description: "View the day",
  prog: "datum dayview",
  usage: `%(prog)s`,
  parents: [dayviewArgs, dbArgs],
});

export type DayviewCmdArgs = MainDatumArgs & {
  nDays?: number;
  startDate?: string;
  endDate?: string;
  width?: number;
  height?: number;
  dayHeight?: number;
  outputFile?: string;
  watch?: boolean;
};

export async function dayviewCmd(
  args: DayviewCmdArgs | string | string[],
  preparsed?: Partial<DayviewCmdArgs>,
) {
  args = parseIfNeeded(dayviewCmdArgs, args, preparsed);
  const db = connectDb(args);

  if (args.watch) {
    console.log("watching");
    const emitter = db.changes({ since: "now", live: true });
    while (true) {
      console.log("redrawing");
      await Promise.all([
        dayview(args),
        // redraw every 5 minutes or when data changes
        new Promise((resolve) => {
          setTimeout(resolve, 1000 * 60 * 5);
          emitter.once("change", resolve);
        }),
      ]);
    }
  }

  await dayview(args);
}

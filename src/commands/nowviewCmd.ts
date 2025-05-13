import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { nowview } from "../dayview/nowview";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { connectDb } from "../auth/connectDb";

export const nowviewArgs = new ArgumentParser({
  add_help: false,
  parents: [],
});

nowviewArgs.add_argument("--width", {
  help: "width of the svg",
  type: "int",
});

nowviewArgs.add_argument("--height", {
  help: "height of the svg",
  type: "int",
});

nowviewArgs.add_argument("--output-file", "-o", {
  help: "output file, should have a .html or .svg extension",
  type: "str",
  dest: "outputFile",
});

nowviewArgs.add_argument("--time-axis-height", {
  help: "height of the time axis",
  type: "int",
  dest: "timeAxisHeight",
});

nowviewArgs.add_argument("--watch", "-w", {
  help: "watch for changes and update every minute",
  action: "store_true",
  dest: "watch",
});

nowviewArgs.add_argument("--history", {
  help: "show history for the specified duration (e.g. 15m, 1h)",
  type: "str",
  dest: "history",
});

nowviewArgs.add_argument("--now-width", {
  help: "width of current status in time units (default: 5m)",
  type: "str",
  dest: "nowWidth",
});

nowviewArgs.add_argument("--margin", {
  help: "margin around the svg",
  type: "int",
  default: 2,
  dest: "margin",
});

export const nowviewCmdArgs = new ArgumentParser({
  description: "View the current state with optional history",
  prog: "datum nowview",
  usage: `%(prog)s`,
  parents: [nowviewArgs, dbArgs],
});

export type NowviewCmdArgs = MainDatumArgs & {
  width?: number;
  height?: number;
  outputFile?: string;
  timeAxisHeight?: number;
  watch?: boolean;
  history?: string;
  nowWidth?: string;
  margin?: number;
};

export async function nowviewCmd(
  args: NowviewCmdArgs | string | string[],
  preparsed?: Partial<NowviewCmdArgs>,
): Promise<string> {
  args = parseIfNeeded(nowviewCmdArgs, args, preparsed);
  const db = connectDb(args);

  if (args.watch) {
    console.log("watching");
    const emitter = db.changes({ since: "now", live: true });
    while (true) {
      console.log("redrawing");
      await Promise.all([
        nowview(args),
        // redraw every minute or when data changes
        new Promise((resolve) => {
          setTimeout(resolve, 1000 * 60);
          emitter.once("change", resolve);
        }),
      ]);
    }
  }

  return await nowview(args);
}

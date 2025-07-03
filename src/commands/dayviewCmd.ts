import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { dayview } from "../dayview/dayview";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { connectDb } from "../auth/connectDb";
import { WatchingDayview } from "../dayview/WatchingDayview";

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

dayviewArgs.add_argument("--end-date", "-d", {
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

dayviewArgs.add_argument("--time-axis-height", {
  help: "height of the time axis",
  type: "int",
  dest: "timeAxisHeight",
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

dayviewArgs.add_argument("--margin", {
  help: "margin around the svg",
  type: "int",
  default: 2,
  dest: "margin",
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
  timeAxisHeight?: number;
  dayHeight?: number;
  outputFile?: string;
  watch?: boolean;
  margin?: number;
};

export async function dayviewCmd(
  args: DayviewCmdArgs | string | string[],
  preparsed?: Partial<DayviewCmdArgs>,
): Promise<string> {
  args = parseIfNeeded(dayviewCmdArgs, args, preparsed);
  const db = connectDb(args);

  if (args.watch) {
    console.log("watching (WatchingDayview)");

    const dayviewWatcher = new WatchingDayview(args);
    try {
      await dayviewWatcher.initialize();
      dayviewWatcher.getOutput();
    } catch (error) {
      console.error("Error during initial render:", error instanceof Error ? error.message : String(error));
      console.log("Continuing to watch for changes...");
    }

    const changedDays = new Set<string>();
    let timeoutId: NodeJS.Timeout | null = null;

    const processChanges = async () => {
      try {
        if (changedDays.size === 0) {
          console.log("periodic redraw (no changes detected)");
          await dayviewWatcher.initialize();
          dayviewWatcher.getOutput();
        } else {
          console.log(
            `redrawing ${changedDays.size} changed day(s): ${Array.from(changedDays).join(", ")}`,
          );
          for (const day of changedDays) {
            await dayviewWatcher.updateDay(day);
          }
          dayviewWatcher.getOutput();
          changedDays.clear();
        }
      } catch (error) {
        console.error("Error during redraw:", error instanceof Error ? error.message : String(error));
      }
    };

    const changes = db.changes({
      since: "now",
      live: true,
      include_docs: true,
    });

    changes.on("change", (change) => {
      console.log("Change detected:", change.id);
      console.log("Change doc:", JSON.stringify(change.doc, null, 2));
      const affectedDay = dayviewWatcher.determineDayFromChange(change);
      if (affectedDay) {
        console.log("Affected day:", affectedDay);
        changedDays.add(affectedDay);

        if (timeoutId) {
          clearTimeout(timeoutId);
        }
        timeoutId = setTimeout(() => {
          processChanges();
          timeoutId = null;
        }, 1000);
      } else {
        console.log("No affected day found for change");
      }
    });

    setInterval(processChanges, 1000 * 60 * 5);

    await new Promise(() => {});
  }

  return await dayview(args);
}

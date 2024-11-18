import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { dayview } from "../dayview/dayview";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const dayviewArgs = new ArgumentParser({
  add_help: false,
  parents: [],
});

dayviewArgs.add_argument("--n-days", {
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
};

export async function dayviewCmd(
  args: DayviewCmdArgs | string | string[],
  preparsed?: Partial<DayviewCmdArgs>,
) {
  args = parseIfNeeded(dayviewCmdArgs, args, preparsed);
  await dayview(args);
  console.log(new Date());
}

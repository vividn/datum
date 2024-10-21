import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { dayview } from "../dayview/dayview";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const dayviewArgs = new ArgumentParser({
  add_help: false,
  parents: [],
});

export const dayviewCmdArgs = new ArgumentParser({
  description: "View the day",
  prog: "datum dayview",
  usage: `%(prog)s`,
  parents: [dayviewArgs, dbArgs],
});

export type DayviewCmdArgs = MainDatumArgs;

export async function dayviewCmd(
  args: DayviewCmdArgs | string | string[],
  preparsed?: Partial<DayviewCmdArgs>,
) {
  args = parseIfNeeded(dayviewCmdArgs, args, preparsed);
  const svg = await dayview(args);
  if (svg !== null) {
    console.log(svg);
  }
}

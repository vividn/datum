import { EitherDocument } from "../documentControl/DatumDocument.js";
import { switchCmd } from "./switchCmd.js";
import { OccurCmdArgs } from "./occurCmd.js";
import { DurationArgs, durationArgs } from "../input/durationArgs.js";
import set from "lodash.set";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded.js";
import { fieldArgs } from "../input/fieldArgs.js";
import { timeArgs } from "../input/timeArgs.js";
import { dataArgs } from "../input/dataArgs.js";
import { dbArgs } from "../input/dbArgs.js";
import { outputArgs } from "../input/outputArgs.js";
import { newDocArgs } from "./addCmd.js";

export const startArgs = new ArgumentParser({
  add_help: false,
  parents: [fieldArgs, durationArgs, newDocArgs, timeArgs, dataArgs],
});

export const startCmdArgs = new ArgumentParser({
  description: "record the start of something that occurs in blocks of time",
  prog: "datum start",
  usage: `%(prog)s <field> [<duration OR .> [data..]]
  %(prog)s <field> -k <reqKey> -k <optKey>=[defaultValue] ... <duration OR .> <reqValue> [optValue] [data..]
`,
  parents: [startArgs, dbArgs, outputArgs],
});

export type StartCmdArgs = OccurCmdArgs & DurationArgs;

export async function startCmd(
  args: StartCmdArgs | string | string[],
  preparsed?: Partial<StartCmdArgs>,
): Promise<EitherDocument> {
  args = parseIfNeeded(startCmdArgs, args, preparsed);
  args.cmdData ??= {};
  set(args.cmdData, "state.id", true);
  return await switchCmd({ ...args });
}

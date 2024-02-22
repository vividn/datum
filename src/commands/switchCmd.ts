import { occurCmd, OccurCmdArgs } from "./occurCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { flexiblePositional } from "../input/flexiblePositional";
import { durationArgs, DurationArgs } from "../input/durationArgs";

import { DatumState } from "../state/normalizeState";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { dataArgs } from "../input/dataArgs";
import { timeArgs } from "../input/timeArgs";
import { fieldArgs } from "../input/fieldArgs";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { newDocArgs } from "./addCmd";

export const stateArgs = new ArgumentParser({
  add_help: false,
});
stateArgs.add_argument("state", {
  help: "the state to switch to, it defaults to true--equivalent to start",
  nargs: "?",
  default: "true",
});
stateArgs.add_argument("--last-state", {
  help: "manually specify the last state being transitioned out of",
  dest: "lastState",
});

export const switchArgs = new ArgumentParser({
  add_help: false,
  parents: [fieldArgs, stateArgs, durationArgs, newDocArgs, timeArgs, dataArgs],
});

export const switchCmdArgs = new ArgumentParser({
  description: "switch states of a given field",
  prog: "dtm switch",
  usage: `%(prog)s <field> [state] [duration] [data..]
  %(prog)s --moment <field> [state] [data..]`,
  parents: [switchArgs, dbArgs, outputArgs],
});

export type StateArgs = {
  state?: DatumState;
  lastState?: DatumState;
};
export type SwitchCmdArgs = OccurCmdArgs & DurationArgs & StateArgs;

export async function switchCmd(
  args: SwitchCmdArgs | string | string[],
  preparsed?: Partial<SwitchCmdArgs>,
): Promise<EitherDocument> {
  args = parseIfNeeded(switchCmdArgs, args, preparsed);
  flexiblePositional(
    args,
    "duration",
    !args.moment && !args.noTimestamp && "optional",
    "dur",
  );
  flexiblePositional(args, "state", "optional");

  args.cmdData ??= {};
  if (args.moment) {
    args.cmdData.dur = null;
  }
  if (args.lastState) {
    args.cmdData.lastState = args.lastState;
  }

  return await occurCmd(args);
}

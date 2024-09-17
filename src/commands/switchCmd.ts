import { occurCmd, OccurCmdArgs } from "./occurCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { flexiblePositional } from "../input/flexiblePositional";
import { durationArgs, DurationArgs } from "../input/durationArgs";

import { DatumState, normalizeState } from "../state/normalizeState";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { dataArgs } from "../input/dataArgs";
import { timeArgs } from "../input/timeArgs";
import { fieldArgs } from "../input/fieldArgs";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { newDocArgs } from "./addCmd";
import { inferType } from "../utils/inferType";

export const stateArgs = new ArgumentParser({
  add_help: false,
});
stateArgs.add_argument("state", {
  help: "the state to switch to, it defaults to true--equivalent to start",
  nargs: "?",
  default: "true",
});
stateArgs.add_argument("--last-state", "-L", {
  help: "manually specify the last state being transitioned out of",
  dest: "lastState",
});

export const switchArgs = new ArgumentParser({
  add_help: false,
  parents: [fieldArgs, stateArgs, durationArgs, newDocArgs, timeArgs, dataArgs],
});

export const switchCmdArgs = new ArgumentParser({
  description: "switch states of a given field",
  prog: "datum switch",
  usage: `%(prog)s <field> <state> [<duration OR .> [data..]]
  %(prog)s --moment <field> <state> [data..]
  %(prog)s <field> -k <reqKey> -k <optKey>=[defaultValue] ... <state> <duration OR .> <reqValue> [optValue] [data..]
`,
  parents: [switchArgs, dbArgs, outputArgs],
});

export type StateArgs = {
  state?: DatumState | string;
  lastState?: DatumState | string;
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
    "dur=",
    args.moment || args.omitTimestamp,
  );
  flexiblePositional(args, "state", "state=true");
  args.cmdData ??= {};
  if (args.moment) {
    args.cmdData.dur = null;
  }
  if (args.lastState) {
    const lastState =
      typeof args.lastState === "string"
        ? inferType(args.lastState)
        : args.lastState;
    if (lastState !== undefined) {
      args.cmdData.lastState = normalizeState(lastState);
    }
  }

  return await occurCmd(args);
}

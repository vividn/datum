import { Argv } from "yargs";
import { occurArgs, occurCmd, OccurCmdArgs } from "./occurCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { flexiblePositional } from "../input/flexiblePositional";
import { durationArgs, DurationArgs } from "../input/durationArgs";

import { DatumState } from "../state/normalizeState";

export const command = [
  "switch <field> [state] [duration] [data..]",
  "switch --moment <field> [state] [data..]",
];
export const desc = "switch states of a given field";

export function builder(yargs: Argv): Argv {
  return durationArgs(occurArgs(yargs))
    .positional("state", {
      describe:
        "the state to switch to, it defaults to true--equivalent to start",
      type: "string",
      nargs: 1,
      default: "true",
    })
    .options({
      "last-state": {
        describe: "manually specify the last state being transitioned out of",
        nargs: 1,
        // TODO: add alias l here after switching lenient to strict
      },
    });
}

export type StateArgs = {
  state?: DatumState;
  lastState?: DatumState;
};
export type SwitchCmdArgs = OccurCmdArgs & DurationArgs & StateArgs;

export async function switchCmd(args: SwitchCmdArgs): Promise<EitherDocument> {
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

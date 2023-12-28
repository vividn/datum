import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { switchCmd } from "./switchCmd";
import { occurArgs, OccurCmdArgs } from "./occurCmd";
import { DurationArgs, durationArgs } from "../input/durationArgs";
import set from "lodash.set";

export const command = "start <field> [duration] [data..]";
export const desc = "add a start document";

export function durOccurArgs(yargs: Argv): Argv {
  return durationArgs(occurArgs(yargs));
}
export function builder(yargs: Argv): Argv {
  return durOccurArgs(yargs);
}

export type StartCmdArgs = OccurCmdArgs & DurationArgs;

export async function startCmd(args: StartCmdArgs): Promise<EitherDocument> {
  args.cmdData ??= {};
  set(args.cmdData, "state.id", true);
  return await switchCmd({ ...args });
}

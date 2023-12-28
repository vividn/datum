import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { durOccurArgs, StartCmdArgs } from "./startCmd";
import { switchCmd } from "./switchCmd";
import set from "lodash.set";

export const command = "end <field> [data..]";
export const desc = "add an end document";

export function builder(yargs: Argv): Argv {
  return durOccurArgs(yargs);
}

export type EndCmdArgs = StartCmdArgs;

export async function endCmd(args: StartCmdArgs): Promise<EitherDocument> {
  args.cmdData ??= {};
  set(args.cmdData, "state.id", false);
  return await switchCmd({ ...args });
}

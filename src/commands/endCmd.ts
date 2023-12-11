import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { durOccurArgs, StartCmdArgs } from "./startCmd";
import { switchCmd } from "./switchCmd";

export const command = "end <field> [data..]";
export const desc = "add an end document";

export function builder(yargs: Argv): Argv {
  return durOccurArgs(yargs);
}

export type EndCmdArgs = StartCmdArgs;

export async function endCmd(args: StartCmdArgs): Promise<EitherDocument> {
  return await switchCmd({ ...args, state: false });
}

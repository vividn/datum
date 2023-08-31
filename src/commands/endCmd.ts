import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { StartCmdArgs } from "./startCmd";
import { switchCmd } from "./switchCmd";
import { baseOccurArgs } from "./occurCmd";

export const command = "end <field> [data..]";
export const desc = "add an end document";

export function builder(yargs: Argv): Argv {
  return baseOccurArgs(yargs);
}

export type EndCmdArgs = StartCmdArgs;

export async function endCmd(args: StartCmdArgs): Promise<EitherDocument> {
  return await switchCmd({ ...args, state: false });
}

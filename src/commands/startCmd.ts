import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { switchCmd } from "./switchCmd";
import { baseOccurArgs, BaseOccurArgs } from "./occurCmd";

export const command = "start <field> [data..]";
export const desc = "add a start document";

export function builder(yargs: Argv): Argv {
  return baseOccurArgs(yargs);
}

export type StartCmdArgs = BaseOccurArgs;

export async function startCmd(args: StartCmdArgs): Promise<EitherDocument> {
  return await switchCmd({ ...args, state: true });
}

import { tailArgs, tailCmd, TailCmdArgs } from "./tailCmd.js";
import { EitherDocument } from "../documentControl/DatumDocument.js";
import { outputArgs } from "../input/outputArgs.js";
import { dbArgs } from "../input/dbArgs.js";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded.js";

export const headCmdArgs = new ArgumentParser({
  description: "show the first occurred/modified/created entries in the db",
  prog: "datum head",
  usage: "%(prog)s [field]",
  parents: [tailArgs, dbArgs, outputArgs],
});

export type HeadCmdArgs = TailCmdArgs;

export async function headCmd(
  args: HeadCmdArgs | string | string[],
  preparsed?: Partial<HeadCmdArgs>,
): Promise<EitherDocument[]> {
  args = parseIfNeeded(headCmdArgs, args, preparsed);
  return tailCmd({ ...args, head: true });
}

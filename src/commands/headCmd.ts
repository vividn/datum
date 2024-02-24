import { tailArgs, tailCmd, TailCmdArgs } from "./tailCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { outputArgs } from "../input/outputArgs";
import { dbArgs } from "../input/dbArgs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const headCmdArgs = new ArgumentParser({
  description: "show the first occurred/modified/created entries in the db",
  prog: "datum head",
  usage: "%(prog)s [field]",
  parents: [tailArgs, dbArgs, outputArgs],
});

export type HeadCmdArgs = TailCmdArgs;

export async function headCmd(
  args: HeadCmdArgs | string | string[],
  preparsed?: Partial<HeadCmdArgs>
): Promise<EitherDocument[]> {
  args = parseIfNeeded(headCmdArgs, args, preparsed);
  return tailCmd({ ...args, head: true });
}

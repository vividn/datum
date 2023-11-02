import { tailCmd, TailCmdArgs, tailCmdYargs } from "./tailCmd";
import { EitherDocument } from "../documentControl/DatumDocument";

export const command = ["head [field]"];
export const desc =
  "show the first occurred/modified/created entries in the db";

export type HeadCmdArgs = TailCmdArgs;
export const headCmdYargs = tailCmdYargs;
export const builder = headCmdYargs;

export async function headCmd(args: HeadCmdArgs): Promise<EitherDocument[]> {
  return tailCmd({ ...args, head: true });
}

import { DataArgs, dataYargs } from "../input/dataArgs";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";

export const command = [
  "undo",
  "undo <field> [data..]",
  "undo -F <data..>",
  "undo <quickId>",
];

export const desc = "Undo a document creation or modification";

export const builder = (yargs: Argv) => {
  return dataYargs(yargs);
}

export type UndoCmdArgs = DataArgs

export async function undoCmd(args: UndoCmdArgs): Promise<{before: EitherDocument, after: EitherDocument}> {

}
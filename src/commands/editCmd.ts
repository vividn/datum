import { quickIdArgs, QuickIdArgs } from "../input/quickIdArg";
import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { quickId, _LAST } from "../ids/quickId";
import { editJSONInTerminal } from "../utils/editInTerminal";
import { overwriteDoc } from "../documentControl/overwriteDoc";
import { MainDatumArgs } from "../input/mainArgs";
import { MyError } from "../errors";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const editArgs = new ArgumentParser({
  add_help: false,
  parents: [quickIdArgs],
});
export const editCmdArgs = new ArgumentParser({
  description: "Edit a document directly with EDITOR",
  prog: "datum edit",
  usage: "%(prog)s <quickId>",
  parents: [editArgs, dbArgs, outputArgs],
});
export type EditCmdArgs = MainDatumArgs & QuickIdArgs;

export class TooManyToEditError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, TooManyToEditError.prototype);
  }
}

export async function editCmd(
  args: EditCmdArgs | string | string[],
  preparsed?: Partial<EditCmdArgs>,
): Promise<EitherDocument> {
  args = parseIfNeeded(editCmdArgs, args, preparsed);
  const db = connectDb(args);

  const ids = await quickId(args.quickId ?? _LAST, args);
  await updateLastDocsRef(db, ids);
  if (ids.length !== 1) {
    throw new TooManyToEditError("Can only edit 1 document at a time");
  }
  const id = ids[0];
  const oldDoc = await db.get(id);
  const newDoc = await editJSONInTerminal(oldDoc);

  const doc = await overwriteDoc({ db, id, payload: newDoc, outputArgs: args });

  return doc;
}

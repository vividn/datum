import { quickIdArgs, QuickIdArgs } from "../input/quickIdArg.js";
import { EitherDocument } from "../documentControl/DatumDocument.js";
import { connectDb } from "../auth/connectDb.js";
import { quickId, _LAST } from "../ids/quickId.js";
import { editJSONInTerminal } from "../utils/editInTerminal.js";
import { overwriteDoc } from "../documentControl/overwriteDoc.js";
import { MainDatumArgs } from "../input/mainArgs.js";
import { MyError } from "../errors.js";
import { updateLastDocsRef } from "../documentControl/lastDocs.js";
import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs.js";
import { outputArgs } from "../input/outputArgs.js";
import { parseIfNeeded } from "../utils/parseIfNeeded.js";

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
  const newDoc = await editJSONInTerminal(oldDoc as EitherDocument);

  const doc = await overwriteDoc({ db, id, payload: newDoc, outputArgs: args });

  return doc;
}

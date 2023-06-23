import { quickIdArg, QuickIdArg } from "../input/quickIdArg";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { quickIds } from "../ids/quickId";
import { editJSONInTerminal } from "../utils/editInTerminal";
import { overwriteDoc } from "../documentControl/overwriteDoc";
import { MainDatumArgs } from "../input/mainYargs";
import { MyError } from "../errors";

export const command = ["edit <quickId>"];
export const desc = "Edit a document directly with EDITOR";

export type EditCmdArgs = MainDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export class TooManyToEditError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, TooManyToEditError.prototype);
  }
}

export async function editCmd(args: EditCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);

  const ids = await quickIds(db, args.quickId);
  if (ids.length !== 1) {
    throw new TooManyToEditError("Can only edit 1 document at a time");
  }
  const id = ids[0];
  const oldDoc = await db.get(id);
  const newDoc = await editJSONInTerminal(oldDoc);

  const doc = await overwriteDoc({ db, id, payload: newDoc, outputArgs: args });

  return doc;
}

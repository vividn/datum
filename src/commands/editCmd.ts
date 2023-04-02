import { quickIdArg, QuickIdArg } from "../input/quickIdArg";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { quickId } from "../ids/quickId";
import { editJSONInTerminal } from "../utils/editInTerminal";
import { overwriteDoc } from "../documentControl/overwriteDoc";
import { MainDatumArgs } from "../input/mainYargs";

export const command = ["edit <quickId>"];
export const desc = "Edit a document directly with EDITOR";

export type EditCmdArgs = MainDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export async function editCmd(args: EditCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);

  const id = await quickId(db, args.quickId);
  const oldDoc = await db.get(id);
  const newDoc = await editJSONInTerminal(oldDoc);

  const doc = await overwriteDoc({ db, id, payload: newDoc, outputArgs: args });

  return doc;
}

import { BaseDatumArgs } from "../input/baseYargs";
import { quickIdArg, QuickIdArg } from "../input/quickIdArg";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { quickId } from "../ids/quickId";
import { Show } from "../output/output";
import { editJSONInTerminal } from "../utils/editInTerminal";
import { overwriteDoc } from "../documentControl/overwriteDoc";

export const command = ["edit <quickId>"];
export const desc = "Edit a document directly with EDITOR";

export type EditCmdArgs = BaseDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export async function editCmd(args: EditCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);

  const id = await quickId(db, args.quickId);
  const oldDoc = await db.get(id);
  const newDoc = await editJSONInTerminal(oldDoc);

  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;

  const doc = await overwriteDoc({ db, id, payload: newDoc, show });

  return doc;
}

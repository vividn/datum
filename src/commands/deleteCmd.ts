import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";
import deleteDoc, { DeletedDocument } from "../documentControl/deleteDoc";
import quickId from "../ids/quickId";
import connectDb from "../auth/connectDb";
import { Show } from "../output/output";
import { QuickIdArg, quickIdArg } from "../input/quickIdArg";

export const command = ["delete <quickId>", "del <quickId>"];
export const desc = "delete a document";

export type DeleteCmdArgs = BaseDatumArgs & QuickIdArg

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export async function deleteCmd(args: DeleteCmdArgs): Promise<DeletedDocument> {
  const db = connectDb(args);
  const id = await quickId(db, args.quickId);
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;

  return await deleteDoc({ id, db, show });
}

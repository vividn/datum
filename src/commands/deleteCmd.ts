import { Argv } from "yargs";
import { deleteDoc, DeletedDocument } from "../documentControl/deleteDoc";
import { quickId } from "../ids/quickId";
import { connectDb } from "../auth/connectDb";
import { QuickIdArg, quickIdArg } from "../input/quickIdArg";
import { MainDatumArgs } from "../input/mainYargs";

export const command = ["delete <quickId>", "del <quickId>"];
export const desc = "delete a document";

export type DeleteCmdArgs = MainDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export async function deleteCmd(args: DeleteCmdArgs): Promise<DeletedDocument> {
  const db = await connectDb(args);
  const id = await quickId(db, args.quickId);

  return await deleteDoc({ id, db, outputArgs: args });
}

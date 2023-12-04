import { Argv } from "yargs";
import { deleteDoc, DeletedDocument } from "../documentControl/deleteDoc";
import { quickIds } from "../ids/quickId";
import { connectDb } from "../auth/connectDb";
import { QuickIdArg, quickIdArg } from "../input/quickIdArg";
import { MainDatumArgs } from "../input/mainYargs";
import { updateLastDocsRef } from "../documentControl/lastDocs";

export const command = ["delete <quickId>", "del <quickId>"];
export const desc = "delete a document";

export type DeleteCmdArgs = MainDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export async function deleteCmd(
  args: DeleteCmdArgs,
): Promise<DeletedDocument[]> {
  const db = connectDb(args);
  const ids = await quickIds(db, args.quickId);

  await updateLastDocsRef(db, ids);
  return await Promise.all(
    ids.map((id) => deleteDoc({ id, db, outputArgs: args })),
  );
}

import { Argv } from "yargs";
import { deleteDoc, DeletedDocument } from "../documentControl/deleteDoc";
import { quickIds } from "../ids/quickId";
import { connectDb } from "../auth/connectDb";
import { QuickIdArg, quickIdArgs } from "../input/quickIdArg";
import { MainDatumArgs } from "../input/mainArgs";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { ArgumentParser } from "argparse";
import { outputArgs } from "../input/outputArgs";
import { dbArgs } from "../input/dbArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const deleteArgs = new ArgumentParser({
  add_help: false,
  parents: [quickIdArgs, dbArgs, outputArgs],
});

export const deleteCmdArgs = new ArgumentParser({
  description: "Delete a document",
  prog: "dtm del[ete]",
  usage: `%(prog)s <quickId>`,
  parents: [deleteArgs],
});

export type DeleteCmdArgs = MainDatumArgs & QuickIdArg;

export async function deleteCmd(
  args: DeleteCmdArgs | string | string[],
  partialArgs?: Partial<DeleteCmdArgs>,
): Promise<DeletedDocument[]> {
  args = parseIfNeeded(deleteCmdArgs, args, partialArgs);
  const db = connectDb(args);
  const ids = await quickIds(db, args.quickId);

  await updateLastDocsRef(db, ids);
  return await Promise.all(
    ids.map((id) => deleteDoc({ id, db, outputArgs: args as DeleteCmdArgs })),
  );
}

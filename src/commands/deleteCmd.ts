import { deleteDoc, DeletedDocument } from "../documentControl/deleteDoc.js";
import { quickId, _LAST_WITH_PROTECTION } from "../ids/quickId.js";
import { connectDb } from "../auth/connectDb.js";
import { QuickIdArgs, quickIdArgs } from "../input/quickIdArg.js";
import { MainDatumArgs } from "../input/mainArgs.js";
import { updateLastDocsRef } from "../documentControl/lastDocs.js";
import { ArgumentParser } from "argparse";
import { outputArgs } from "../input/outputArgs.js";
import { dbArgs } from "../input/dbArgs.js";
import { parseIfNeeded } from "../utils/parseIfNeeded.js";

export const deleteArgs = new ArgumentParser({
  add_help: false,
  parents: [quickIdArgs, dbArgs, outputArgs],
});

export const deleteCmdArgs = new ArgumentParser({
  description: "Delete a document",
  prog: "datum del[ete]",
  usage: `%(prog)s <quickId>`,
  parents: [deleteArgs],
});

export type DeleteCmdArgs = MainDatumArgs & QuickIdArgs;

export async function deleteCmd(
  args: DeleteCmdArgs | string | string[],
  partialArgs?: Partial<DeleteCmdArgs>,
): Promise<DeletedDocument[]> {
  args = parseIfNeeded(deleteCmdArgs, args, partialArgs);
  const db = connectDb(args);
  const ids = await quickId(args.quickId ?? _LAST_WITH_PROTECTION, args);

  await updateLastDocsRef(db, ids);
  return await Promise.all(
    ids.map((id) => deleteDoc({ id, db, outputArgs: args as DeleteCmdArgs })),
  );
}

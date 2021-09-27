import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";
import deleteDoc, { DeletedDocument } from "../documentControl/deleteDoc";
import quickId from "../ids/quickId";
import connectDb from "../auth/connectDb";
import { Show } from "../output";

export const command = "delete <quickId>";
export const desc = "delete a document";

export type DeleteCmdArgs = BaseDatumArgs & {
  quickId: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.positional("quickId", {
    describe:
      "Can be all or the first few letters of the _id or " +
      "the humanId of a document. It must match exactly one document " +
      "unambiguosly.",
    type: "string",
  });
}

export async function deleteCmd(
  args: DeleteCmdArgs
): Promise<DeletedDocument> {
  const db = connectDb(args);
  const id = await quickId(db, args.quickId);
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;

  return await deleteDoc({id, db, show });
}

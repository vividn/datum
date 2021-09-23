import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";
import { DeletedDocument } from "../documentControl/deleteDoc";

export const command = "delete <quickId>";
export const desc = "delete a document";

export type DeleteCmdArgs = BaseDatumArgs & {
  quickId: string;
}

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("quickId", {
      describe: "Can be all or the first few letters of the _id or " +
        "the humanId of a document. It must match exactly one document " +
        "unambiguosly.",
      type: "string"
    });
}

export async function deleteCmd(args: DeleteCmdArgs): Promise<DeletedDocument> {

}
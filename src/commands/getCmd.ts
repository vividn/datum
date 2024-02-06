import { quickIdArgs, QuickIdArg } from "../input/quickIdArg";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { quickIds } from "../ids/quickId";
import { connectDb } from "../auth/connectDb";
import { showExists } from "../output/output";
import { MainDatumArgs } from "../input/mainYargs";
import { updateLastDocsRef } from "../documentControl/lastDocs";

export const command = ["get <quickId>", "see <quickId>"];
export const desc = "display a document";

export type GetCmdArgs = MainDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArgs(yargs);
}

export async function getCmd(args: GetCmdArgs): Promise<EitherDocument[]> {
  const db = connectDb(args);
  const ids = await quickIds(db, args.quickId);

  const docs = await Promise.all(ids.map((id) => db.get(id)));
  docs.forEach((doc) => showExists(doc, args));
  await updateLastDocsRef(db, ids);
  return docs;
}

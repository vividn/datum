import { quickIdArgs, QuickIdArg } from "../input/quickIdArg";
import { EitherDocument } from "../documentControl/DatumDocument";
import { quickId } from "../ids/quickId";
import { connectDb } from "../auth/connectDb";
import { showExists } from "../output/output";
import { MainDatumArgs } from "../input/mainArgs";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { dbArgs } from "../input/dbArgs";
import { OutputArgs, outputArgs } from "../input/outputArgs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const command = ["get <quickId>", "see <quickId>"];
export const desc = "display a document";

export type GetCmdArgs = MainDatumArgs & OutputArgs & QuickIdArg;

export const getCmdArgs = new ArgumentParser({
  description: "display a document",
  prog: "dtm get",
  usage: `%(prog)s <quickId>`,
  parents: [quickIdArgs, dbArgs, outputArgs],
});

export async function getCmd(
  args: GetCmdArgs | string | string[],
  preparsed?: Partial<GetCmdArgs>,
): Promise<EitherDocument[]> {
  args = parseIfNeeded(getCmdArgs, args, preparsed);
  const db = connectDb(args);
  const ids = await quickId(db, args.quickId);

  const docs = await Promise.all(ids.map((id) => db.get(id)));
  docs.forEach((doc) => showExists(doc, args as GetCmdArgs));
  await updateLastDocsRef(db, ids);
  return docs;
}

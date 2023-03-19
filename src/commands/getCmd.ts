import { quickIdArg, QuickIdArg } from "../input/quickIdArg";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { quickId } from "../ids/quickId";
import { connectDb } from "../auth/connectDb";
import { showExists } from "../output/output";
import { MainDatumArgs } from "../input/mainYargs";

export const command = ["get <quickId>", "see <quickId>"];
export const desc = "display a document";

export type GetCmdArgs = MainDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export async function getCmd(args: GetCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);
  const id = await quickId(db, args.quickId);

  const doc = await db.get(id);
  showExists(doc, args);
  return doc;
}

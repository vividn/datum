import { BaseDatumArgs } from "../input/baseYargs";
import { quickIdArg, QuickIdArg } from "../input/quickIdArg";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { quickId } from "../ids/quickId";
import { connectDb } from "../auth/connectDb";
import { Show, showExists } from "../output/output";

export const command = ["get <quickId>", "see <quickId>"];
export const desc = "display a document";

export type GetCmdArgs = BaseDatumArgs & QuickIdArg;

export function builder(yargs: Argv): Argv {
  return quickIdArg(yargs);
}

export async function getCmd(args: GetCmdArgs): Promise<EitherDocument> {
  const db = await connectDb(args);
  const id = await quickId(db, args.quickId);
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;

  const doc = await db.get(id);
  showExists(doc, show);
  return doc;
}

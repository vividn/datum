import { MainDatumArgs } from "../input/mainArgs.js";
import { EitherDocument } from "../documentControl/DatumDocument.js";
import { connectDb } from "../auth/connectDb.js";
import { showExists } from "../output/output.js";
import { updateLastDocsRef } from "../documentControl/lastDocs.js";
import { outputArgs } from "../input/outputArgs.js";
import { dbArgs } from "../input/dbArgs.js";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded.js";

export const grepArgs = new ArgumentParser({
  add_help: false,
});
grepArgs.add_argument("patterns", {
  help: "pattern to match",
  nargs: "+",
});

export const grepCmdArgs = new ArgumentParser({
  description: "Find all docs that match pattern in the database",
  prog: "datum grep",
  usage: "%(prog)s <patterns..>",
  parents: [grepArgs, dbArgs, outputArgs],
});

export type GrepCmdArgs = MainDatumArgs & {
  patterns: string[];
};

export async function grepCmd(
  args: GrepCmdArgs | string | string[],
  preparsed?: Partial<GrepCmdArgs>,
): Promise<EitherDocument[]> {
  const parsed = parseIfNeeded(grepCmdArgs, args, preparsed);
  const db = connectDb(parsed);
  const allDocs = (await db.allDocs({ include_docs: true })).rows;
  const matchingDocs: EitherDocument[] = allDocs.reduce(
    (accum: EitherDocument[], row) => {
      const docString = JSON.stringify(row.doc);
      if (parsed.patterns.every((pattern) => docString.match(pattern))) {
        accum.push(row.doc as EitherDocument);
      }
      return accum;
    },
    [],
  );
  const ids = matchingDocs.map((doc) => {
    showExists(doc, parsed);
    return doc._id;
  });
  await updateLastDocsRef(db, ids);
  return matchingDocs;
}

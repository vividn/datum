import { MainDatumArgs } from "../input/mainArgs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { showExists } from "../output/output";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { outputArgs } from "../input/outputArgs";
import { dbArgs } from "../input/dbArgs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

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

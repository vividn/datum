import { MainDatumArgs } from "../input/mainYargs";
import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { showExists } from "../output/output";

export const command = "grep <patterns..>";
export const describe = "Find all docs that match pattern in the database";

export type GrepCmdArgs = MainDatumArgs & {
  patterns: string[];
};

export function grepCmdYargs(yargs: Argv): Argv {
  return yargs.positional("patterns", {
    describe: "pattern to match",
    type: "string",
  });
}

export const builder = grepCmdYargs;

export async function grepCmd(args: GrepCmdArgs): Promise<EitherDocument[]> {
  const db = await connectDb(args);
  const allDocs = (await db.allDocs({ include_docs: true })).rows;
  const matchingDocs: EitherDocument[] = allDocs.reduce(
    (accum: EitherDocument[], row) => {
      const docString = JSON.stringify(row.doc);
      if (args.patterns.every((pattern) => docString.match(pattern))) {
        accum.push(row.doc as EitherDocument);
      }
      return accum;
    },
    []
  );
  matchingDocs.forEach((doc) => {
    showExists(doc, args);
  });
  return matchingDocs;
}

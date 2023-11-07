import { MainDatumArgs } from "../input/mainYargs";
import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { readFileSync } from "fs";
import { brotliDecompressSync } from "zlib";

export const command = "restore <filename>";

export const desc =
  "Restore db from backup file, for now only use on a new empty db for best results";

export type RestoreCmdArgs = MainDatumArgs & {
  filename: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.positional("filename", {
    type: "string",
    args: 1,
  });
}

export async function restoreCmd(args: RestoreCmdArgs): Promise<void> {
  const db = connectDb(args);
  const buffer = await readFileSync(args.filename);
  const decompressed = await brotliDecompressSync(buffer);
  const allDocs = JSON.parse(decompressed.toString());
  // await db.bulkDocs(allDocs, { new_edits: false });
  console.log({allDocs});
}

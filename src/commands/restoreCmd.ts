import { connectDb } from "../auth/connectDb";
import { readFileSync } from "fs";
import { dbArgs } from "../input/dbArgs";
import { ArgumentParser } from "argparse";
import { BackupCmdArgs } from "./backupCmd";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { MainDatumArgs } from "../input/mainArgs";

export const restoreArgs = new ArgumentParser({
  add_help: false,
});
restoreArgs.add_argument("filename", {
  help: "file to restore from",
  type: "str",
});
restoreArgs.add_argument("--allow-nonempty", {
  help: "Allow restore even if the db is not empty",
  action: "store_true",
  dest: "allowNonempty",
});

export const restoreCmdArgs = new ArgumentParser({
  description:
    "Restore db from backup file, for now only use on a new empty db for best results",
  prog: "datum restore",
  usage: "%(prog)s <filename>",
  parents: [restoreArgs, dbArgs],
});
export type RestoreCmdArgs = MainDatumArgs & {
  filename: string;
  allowNonempty?: boolean;
};
export async function restoreCmd(
  args: RestoreCmdArgs | string | string[],
  preparsed?: Partial<BackupCmdArgs>
): Promise<void> {
  args = parseIfNeeded(restoreCmdArgs, args, preparsed);
  args.createDb ??= true;
  const db = connectDb(args);
  const info = await db.info();
  if (!args.allowNonempty && info.doc_count !== 0) {
    throw new Error(
      "Warning: db is not empty, aborting restore. Use --allow-nonempty to override."
    );
  }
  const buffer = readFileSync(args.filename);
  const allDocs = JSON.parse(buffer.toString()).docs;
  await db.bulkDocs(allDocs, { new_edits: false });
}

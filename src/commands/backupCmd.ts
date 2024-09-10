import { MainDatumArgs } from "../input/mainArgs";
import { connectDb } from "../auth/connectDb";
import * as fs from "fs";
import { DateTime } from "luxon";
import { dbArgs } from "../input/dbArgs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const backupArgs = new ArgumentParser({
  add_help: false,
});
// TODO: make this optional and then use stdout if not specified
backupArgs.add_argument("filename", {
  help: "file to write the backup to",
  type: "str",
});
backupArgs.add_argument("--overwrite", {
  help: "overwrite file if it exists",
  action: "store_true",
});

export const backupCmdArgs = new ArgumentParser({
  description: "Backup db, outputs to stdout",
  prog: "datum backup",
  usage: "%(prog)s <filename>",
  parents: [backupArgs, dbArgs],
});

export type BackupCmdArgs = MainDatumArgs & {
  filename: string;
  overwrite?: boolean;
};

export async function backupCmd(
  args: BackupCmdArgs | string | string[],
  preparsed?: Partial<BackupCmdArgs>,
): Promise<void> {
  args = parseIfNeeded(backupCmdArgs, args, preparsed);
  if (!args.overwrite && fs.existsSync(args.filename)) {
    throw new Error("File exists, overwrite with --overwrite");
  }
  const db = connectDb(args);
  const allDocs = (await db.allDocs({ include_docs: true })).rows.map(
    ({ doc }) => doc,
  );
  // TODO: Also backup and restore attachments (even though using attachments is considered not best practice)
  const backupTime = DateTime.utc().toISO() as string;
  const backupData = JSON.stringify({ backupTime, docs: allDocs }, null, 0);
  fs.writeFileSync(args.filename, backupData);
}

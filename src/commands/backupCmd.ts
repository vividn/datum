import { Argv } from "yargs";
import { MainDatumArgs } from "../input/mainYargs";
import { connectDb } from "../auth/connectDb";
import { createWriteStream } from "fs";
import * as fs from "fs";
import { DateTime } from "luxon";

export const command = "backup <filename>";
export const desc = "Backup db, outputs to stdout";

export type BackupCmdArgs = MainDatumArgs & {
  filename: string;
  overwrite?: boolean;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("filename", {
      type: "string",
      args: 1,
    })
    .options({
      overwrite: {
        type: "boolean",
        default: false,
      },
    });
}

export async function backupCmd(args: BackupCmdArgs): Promise<void> {
  if (!args.overwrite && fs.existsSync(args.filename)) {
    throw new Error("File exists, overwrite with --overwrite");
  }
  const db = connectDb(args);
  const allDocs = (await db.allDocs({ include_docs: true })).rows.map(
    ({ doc }) => doc
  );
  // TODO: Also backup and restore attachments (even though using attachments is considered not best practice)
  const backupTime = DateTime.utc().toISO() as string;
  const buffer = Buffer.from(
    JSON.stringify({ backupTime, docs: allDocs }, null, 0)
  );
  fs.writeFileSync(args.filename, buffer);
}

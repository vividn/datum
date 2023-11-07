import { Argv } from "yargs";
import { MainDatumArgs } from "../input/mainYargs";
import { connectDb } from "../auth/connectDb";
import { createWriteStream } from "fs";
import { brotliCompressSync } from "zlib";

export const command = "backup <filename>";
export const desc = "Backup db, outputs to stdout";

export type BackupCmdArgs = MainDatumArgs & {
  filename: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.positional("filename", {
    type: "string",
    args: 1,
  });
}

export async function backupCmd(args: BackupCmdArgs): Promise<void> {
  const writeStream = createWriteStream(args.filename);
  const db = connectDb(args);
  const allDocs = (await db.allDocs({ include_docs: true })).rows.map(
    ({ doc }) => doc
  );
  const buffer = Buffer.from(JSON.stringify(allDocs, null, 0));
  const compressed = await brotliCompressSync(buffer);
  await writeStream.write(compressed);
}

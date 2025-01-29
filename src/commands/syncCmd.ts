import PouchDb from "pouchdb";
import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { connectDb } from "../auth/connectDb";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { jClone } from "../utils/jClone";

export const syncArgs = new ArgumentParser({
  add_help: false,
  parents: [dbArgs, outputArgs],
});
syncArgs.add_argument("remote", {
  help: "the remote host to sync with",
  nargs: 1,
});
syncArgs.add_argument("--watch", "-w", {
  help: "watch for changes and sync continuously",
  action: "store_true",
});

export const syncCmdArgs = new ArgumentParser({
  description: "Sync data with a remote host",
  prog: "datum sync",
  usage: `%(prog)s remote`,
  parents: [syncArgs],
});

export type SyncCmdArgs = MainDatumArgs & {
  remote: string;
  watch?: boolean;
};

export async function syncCmd(
  argsOrCli: SyncCmdArgs | string | string[],
  preparsed?: Partial<SyncCmdArgs>,
): Promise<void> {
  const args = parseIfNeeded(syncCmdArgs, argsOrCli, preparsed);
  const db = connectDb(args);

  // TODO: allow using different username/password for remote
  const couchAuth = {
    username: args.user,
    password: args.password,
  }
  const remote = args.remote;
  const remoteDb = new PouchDb(remote, { auth: couchAuth });

  const watch = args.watch;

  if (watch) {
    await db.sync(remoteDb, { live: true, retry: true });
  } else {
    await db.sync(remoteDb);
  }
}

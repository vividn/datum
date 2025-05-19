import PouchDb from "pouchdb";
import levelAdapter from "pouchdb-adapter-leveldb";
import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";

PouchDb.plugin(levelAdapter);

export function connectDbFile(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  const { db: dbName = "datum", createDb } = args;

  let host = args.host;

  if (host === undefined || host === "") {
    const dataDir =
      process.env["XDG_DATA_HOME"] ?? process.env["HOME"] + "/.local/share";
    host = `${dataDir}/datum`;
  }

  const fullDatabaseName = !host
    ? dbName
    : host.at(-1) === "/"
      ? `${host}${dbName}`
      : `${host}/${dbName}`;

  if (host.startsWith("/")) {
    const fs = eval("require('fs')");
    // create parent directories
    if (!fs.existsSync(fullDatabaseName)) {
      fs.mkdirSync(fullDatabaseName, { recursive: true });
    }
  }

  const couchAuth = {
    username: args.user,
    password: args.password,
  };
  return new PouchDb(fullDatabaseName, {
    skip_setup: !createDb,
    auth: couchAuth,
    adapter: "leveldb",
  });
}

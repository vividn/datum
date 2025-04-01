import { EitherPayload } from "../documentControl/DatumDocument";
import PouchDb from "pouchdb";
import memoryAdapter from "pouchdb-adapter-memory";
import { MainDatumArgs } from "../input/mainArgs";
import { mergeConfigAndEnvIntoArgs } from "../config/mergeConfigIntoArgs";
import fs from "fs";

PouchDb.plugin(memoryAdapter);

export function connectDb(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  mergeConfigAndEnvIntoArgs(args);

  let host = args.host;
  const adapter =
    process.env["POUCHDB_ADAPTER"] || host === "%MEMORY%"
      ? "memory"
      : undefined;
  const { db: dbName = "datum", createDb } = args;

  if (host === undefined || host === "") {
    if (typeof window !== "undefined") {
      // just use database name if in the browser
      host = "";
    } else {
      const dataDir =
        process.env["XDG_DATA_HOME"] ?? process.env["HOME"] + "/.local/share";
      host = `${dataDir}/datum`;
    }
  }

  const fullDatabaseName =
    adapter === "memory"
      ? dbName
      : !host
        ? dbName
        : host.at(-1) === "/"
          ? `${host}${dbName}`
          : `${host}/${dbName}`;

  if (host.startsWith("/") && adapter !== "memory") {
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
    adapter,
  });
}

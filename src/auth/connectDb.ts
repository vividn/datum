import { EitherPayload } from "../documentControl/DatumDocument";
import PouchDb from "pouchdb";
import memoryAdapter from "pouchdb-adapter-memory";
import { MainDatumArgs } from "../input/mainArgs";
import { mergeConfigAndEnvIntoArgs } from "../config/mergeConfigIntoArgs";

PouchDb.plugin(memoryAdapter);

export function connectDb(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  mergeConfigAndEnvIntoArgs(args);

  const host = args.host;
  const adapter =
    process.env["POUCHDB_ADAPTER"] || host === "%MEMORY%"
      ? "memory"
      : undefined;
  const { db: dbName = "datum", createDb } = args;

  if (host === undefined) {
    // TODO: when going into the browser, maybe support this?
    throw new Error("No hostname provided for database connection");
  }

  const fullDatabaseName =
    adapter === "memory"
      ? dbName
      : !host
        ? dbName
        : host.at(-1) === "/"
          ? `${host}${dbName}`
          : `${host}/${dbName}`;

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

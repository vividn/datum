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

  const hostname = args.host;
  const adapter = hostname === "%MEMORY%" ? "memory" : undefined;
  const { db: dbName = "datum", createDb } = args;

  const fullDatabaseName =
    adapter === "memory"
      ? dbName
      : !hostname
        ? dbName
        : hostname.at(-1) === "/"
          ? `${hostname}${dbName}`
          : `${hostname}/${dbName}`;

  const couchAuth = {
    username: args.user,
    password: args.password
  };
  return new PouchDb(fullDatabaseName, {
    skip_setup: !createDb,
    auth: couchAuth,
    adapter,
  });
}

import PouchDb from "pouchdb";
import httpAdapter from "pouchdb-adapter-http";
import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";

PouchDb.plugin(httpAdapter);

export function connectDbHttp(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  const { db: dbName = "datum", createDb } = args;
  const host = args.host;

  if (!host) {
    throw new Error("Host must be specified for HTTP connection.");
  }

  const fullDatabaseName =
    host.at(-1) === "/" ? `${host}${dbName}` : `${host}/${dbName}`;

  const couchAuth = {
    username: args.user,
    password: args.password,
  };
  return new PouchDb(fullDatabaseName, {
    skip_setup: !createDb,
    auth: couchAuth,
    adapter: "http",
  });
}

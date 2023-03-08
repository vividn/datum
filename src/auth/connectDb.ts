import { EitherPayload } from "../documentControl/DatumDocument";
import dotenv from "dotenv";
import { MainDatumArgs } from "../input/mainYargs";
import PouchDb from "pouchdb";

export async function connectDb(
  args: MainDatumArgs
): Promise<PouchDB.Database<EitherPayload>> {
  if (
    process.env.NODE_ENV?.includes("dev") ||
    process.env.NODE_ENV?.includes("test")
  ) {
    process.env.COUCHDB_USER = "admin";
    process.env.COUCHDB_PASSWORD = "password";
    process.env.COUCHDB_HOSTNAME = "localhost:5983";
  }
  if (args.env !== undefined) {
    dotenv.config({ path: args.env, override: true });
  }

  process.env.COUCHDB_HOSTNAME ??= "locahost:5984";

  const couchConfig = {
    username:
      args.username ??
      process.env.COUCHDB_USER ??
      (() => {
        throw new Error(
          "No username set. Specify with --username, or COUCHDB_USER"
        );
      })(),
    password:
      args.password ??
      process.env.COUCHDB_PASSWORD ??
      (() => {
        throw new Error(
          "No password set. Specify with --password or COUCHDB_PASSWORD"
        );
      })(),
    hostname:
      args.host ??
      process.env.COUCHDB_HOSTNAME ??
      (() => {
        throw new Error(
          "No hostame set. Specify with --host or COUCHDB_HOSTNAME"
        );
      })(),
  };
  const { db: dbName = "datum", createDb } = args;

  return new PouchDb(
    `http://${couchConfig.username}:${couchConfig.password}@${couchConfig.hostname}/${dbName}`,
    { skip_setup: !createDb }
  );
}

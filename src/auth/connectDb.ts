import Nano, { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import dotenv from "dotenv";
import { pass } from "../utils/pass";
import { MainDatumArgs } from "../input/mainYargs";

export function connectNano(args: MainDatumArgs): Nano.ServerScope {
  if (
    process.env.NODE_ENV?.includes("dev") ||
    process.env.NODE_ENV?.includes("test")
  ) {
    process.env.COUCHDB_USER = "admin";
    process.env.COUCHDB_PASSWORD = "password";
    process.env.COUCHDB_HOSTNAME = "localhost:5983";
  }
  if (args.env !== undefined) {
    dotenv.config({ path: args.env });
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
  return Nano(
    `http://${couchConfig.username}:${couchConfig.password}@${couchConfig.hostname}`
  );
}

export async function connectDb(
  args: MainDatumArgs
): Promise<DocumentScope<EitherPayload>> {
  const nano = connectNano(args);
  const { db: dbName = "datum" } = args;
  if (args.createDb) {
    await nano.db.create(dbName).catch(pass);
  }
  const db: DocumentScope<EitherPayload> = nano.use(dbName);
  return db;
}

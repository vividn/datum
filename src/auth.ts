import pass from "./utils/pass";
import Nano, { DocumentScope } from "nano";
import { EitherPayload } from "./documentControl/DatumDocument";
import dotenv from "dotenv";
import { BaseDatumArgs } from "./input";

export const connectDb = async (
  args: BaseDatumArgs
): Promise<DocumentScope<EitherPayload>> => {
  if (args.env !== undefined) {
    dotenv.config({ path: args.env });
  }
  const env = process.env.NODE_ENV || "production";
  const defaultHost =
    env === "production" ? "localhost:5984" : "localhost:5983";
  const couchConfig = {
    username: args.username ?? process.env.COUCHDB_USER ?? "admin",
    password: args.password ?? process.env.COUCHDB_PASSWORD ?? "password",
    hostname: args.host ?? process.env.COUCHDB_HOSTNAME ?? defaultHost,
  };
  const nano = Nano(
    `http://${couchConfig.username}:${couchConfig.password}@${couchConfig.hostname}`
  );

  const { db: dbName = "datum" } = args;

  if (args.autoCreateDb) {
    // Create database if it doesn't exist
    // TODO: Remove this
    await nano.db.create(dbName).catch(pass);
  }

  const db: DocumentScope<EitherPayload> = nano.use(dbName);
  return db;
};

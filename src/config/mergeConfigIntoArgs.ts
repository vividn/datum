import { MainDatumArgs } from "../input/mainArgs";
import dotenv from "dotenv";
import { loadConfig } from "./loadConfig";

export function mergeConfigAndEnvIntoArgs(args: MainDatumArgs): void {
  // TODO: have settings for use in the browser and load them here
  if (window !== undefined) {
    return;
  }
  
  if (args.env !== undefined) {
    dotenv.config({ path: args.env, override: true });
  }
  const config = loadConfig(args);

  args.db ??= config.db;

  args.host ??= process.env["DATUM_HOST"] || config?.host;
  args.user ??= process.env["COUCHDB_USER"] || config?.user;
  args.password ??=
    (process.env["COUCHDB_PASSWORD"] || config?.password) ?? undefined;
}

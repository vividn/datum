import { MainDatumArgs } from "../input/mainArgs";
import dotenv from "dotenv";
import { loadConfig } from "./loadConfig";

export async function mergeConfigAndEnvIntoArgs(
  args: MainDatumArgs,
): Promise<void> {
  if (args.env !== undefined) {
    dotenv.config({ path: args.env, override: true });
  }
  const config = await loadConfig(args);

  args.db ??= config.db;

  args.host ??= process.env["COUCHDB_HOST"] || config?.host;
  args.user ??= process.env["COUCHDB_USER"] || config?.user;
  args.password ??=
    (process.env["COUCHDB_PASSWORD"] || config?.password) ?? undefined;
}

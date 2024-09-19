import { MainDatumArgs } from "../input/mainArgs";
import dotenv from "dotenv";
import { loadConfig } from "./loadConfig";
import { SetupCmdArgs } from "../commands/setupCmd";

export function mergeConfigAndEnvIntoArgs(
  args: MainDatumArgs & SetupCmdArgs,
): void {
  if (args.env !== undefined) {
    dotenv.config({ path: args.env, override: true });
  }
  const config = loadConfig(args);

  args.projectDir ??= config.project_dir;
  args.db ??= config.db;

  console.debug({
    argsHost: args.host,
    envHost: process.env["COUCHDB_HOST"],
    configHost: config.connection?.host,
  });
  args.host ??= process.env["COUCHDB_HOST"] || config.connection?.host;
  args.user ??= process.env["COUCHDB_USER"] || config.connection?.user;
  args.password ??=
    (process.env["COUCHDB_PASSWORD"] || config.connection?.password) ??
    undefined;
}

import { MainDatumArgs } from "../input/mainArgs";
import dotenv from "dotenv";
import { loadConfig } from "./loadConfig";
import { SetupCmdArgs } from "../commands/setupCmd";

export async function mergeConfigAndEnvIntoArgs(
  args: MainDatumArgs & SetupCmdArgs,
): Promise<void> {
  if (args.env !== undefined) {
    dotenv.config({ path: args.env, override: true });
  }
  const config = await loadConfig(args);

  args.projectDir ??= config.project_dir;
  args.db ??= config.db;

  args.host ??= process.env["COUCHDB_HOST"] || config.connection?.host;
  args.user ??= process.env["COUCHDB_USER"] || config.connection?.user;
  args.password ??=
    process.env["COUCHDB_PASSWORD"] || config.connection?.password;
}

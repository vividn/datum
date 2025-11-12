import { migrateEditCmdArgs } from "./migrate/migrateEditCmd.js";
import { MainDatumArgs } from "../input/mainArgs.js";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded.js";
import { migrateInteractiveCmdArgs } from "./migrate/migrateInteractiveCmd.js";
import { migrateRunCmdArgs } from "./migrate/migrateRunCmd.js";
import { dbArgs } from "../input/dbArgs.js";
import { outputArgs } from "../input/outputArgs.js";

export const migrateArgs = new ArgumentParser({
  add_help: false,
  parents: [dbArgs, outputArgs],
});
const subparsers = migrateArgs.add_subparsers({
  title: "subcommands",
});
subparsers.add_parser("edit", {
  aliases: ["add"],
  description: "add or edit a migration",
  parents: [migrateEditCmdArgs],
});
subparsers.add_parser("run", {
  description: "Run a migration",
  parents: [migrateRunCmdArgs],
});
subparsers.add_parser("interactive", {
  aliases: ["i"],
  description: "Run a migration interactively",
  parents: [migrateInteractiveCmdArgs],
});

export const migrateCmdArgs = new ArgumentParser({
  description: "migrate data from one state to another",
  prog: "datum migrate",
  usage: "%(prog)s <subcommand>",
  parents: [migrateArgs],
});

export type MigrateCmdArgs = MainDatumArgs & {
  subfn: (
    args: MigrateCmdArgs,
    preparsed?: Partial<MigrateCmdArgs>,
  ) => Promise<unknown>;
};

export async function migrateCmd(
  args: MigrateCmdArgs | string | string[],
  preparsed?: Partial<MigrateCmdArgs>,
): Promise<unknown> {
  args = parseIfNeeded(migrateCmdArgs, args, preparsed, true);
  return await args.subfn(args);
}

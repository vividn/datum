import { migrateEditCmdArgs } from "./migrate/migrateEditCmd";
import { MainDatumArgs } from "../input/mainArgs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const migrateArgs = new ArgumentParser({
  add_help: false,
});
const subparsers = migrateArgs.add_subparsers({
  title: "subcommands",
});
subparsers.add_parser("edit", {
  aliases: ["add"],
  description: "add or edit a migration",
  parents: [migrateEditCmdArgs],
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
    preparsed?: Partial<MigrateCmdArgs>
  ) => Promise<unknown>;
};

export async function migrateCmd(
  args: MigrateCmdArgs | string | string[],
  preparsed?: Partial<MigrateCmdArgs>
): Promise<unknown> {
  args = parseIfNeeded(migrateCmdArgs, args, preparsed);
  return await args.subfn(args);
}

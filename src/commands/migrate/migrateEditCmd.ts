import { MigrateCmdArgs } from "../migrateCmd.js";
import { connectDb } from "../../auth/connectDb.js";
import { editMigration } from "../../migrations/editMigration.js";
import { MapFunction } from "../../views/DatumView.js";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../../utils/parseIfNeeded.js";

export const migrateEditArgs = new ArgumentParser({
  add_help: false,
});
migrateEditArgs.add_argument("name", {
  help: "name of the migration",
  type: "str",
});
migrateEditArgs.add_argument("--function", {
  help: "map function string to use for the migration, rather than editing in terminal",
  type: "str",
});
migrateEditArgs.set_defaults({ subfn: migrateEditCmd });

export const migrateEditCmdArgs = new ArgumentParser({
  description: "add or edit a migration",
  prog: "datum migrate edit",
  usage: "%(prog)s <name>",
  parents: [migrateEditArgs],
  add_help: false,
});

export type MigrateEditCmdArgs = MigrateCmdArgs & {
  name: string;
  function: MapFunction | string;
};

export async function migrateEditCmd(
  args: MigrateEditCmdArgs | string | string[],
  preparsed?: Partial<MigrateEditCmdArgs>,
): Promise<void> {
  args = parseIfNeeded(migrateEditCmdArgs, args, preparsed);
  const db = connectDb(args);

  return await editMigration({
    db,
    name: args.name,
    mapFn: args.function,
    outputArgs: args,
  });
}

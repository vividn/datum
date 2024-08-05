import { MigrateCmdArgs } from "../migrateCmd";
import { connectDb } from "../../auth/connectDb";
import { MapFunction } from "../../views/DatumView";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../../utils/parseIfNeeded";
import { runMigration } from "../../migrations/runMigration";

export const migrateRunArgs = new ArgumentParser({
  add_help: false,
});
migrateRunArgs.add_argument("name", {
  help: "name of the migration",
  type: "str",
});
migrateRunArgs.set_defaults({ subfn: migrateRunCmd });

export const migrateRunCmdArgs = new ArgumentParser({
  description: "Run a migration. Rows with the same key are processed in parallel",
  prog: "datum migrate run",
  usage: "%(prog)s <name>",
  parents: [migrateRunArgs],
  add_help: false,
});

export type MigrateRunCmdArgs = MigrateCmdArgs & {
  name: string;
  function: MapFunction | string;
};

export async function migrateRunCmd(
  args: MigrateRunCmdArgs | string | string[],
  preparsed?: Partial<MigrateRunCmdArgs>,
): Promise<void> {
  args = parseIfNeeded(migrateRunCmdArgs, args, preparsed);
  const db = connectDb(args);

  return await runMigration({
    db,
    name: args.name,
    outputArgs: args,
  });
}

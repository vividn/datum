import { MigrateCmdArgs } from "../migrateCmd";
import { connectDb } from "../../auth/connectDb";
import { MapFunction } from "../../views/DatumView";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../../utils/parseIfNeeded";
import { interactiveMigration } from "../../migrations/interactiveMigration";

export const migrateInteractiveArgs = new ArgumentParser({
  add_help: false,
});
migrateInteractiveArgs.add_argument("name", {
  help: "name of the migration",
  type: "str",
});
migrateInteractiveArgs.set_defaults({ subfn: migrateInteractiveCmd });

export const migrateInteractiveCmdArgs = new ArgumentParser({
  description: "Run a migration interactively",
  prog: "datum migrate interactive",
  usage: "%(prog)s <name>",
  parents: [migrateInteractiveArgs],
  add_help: false,
});

export type MigrateInteractiveCmdArgs = MigrateCmdArgs & {
  name: string;
  function: MapFunction | string;
};

export async function migrateInteractiveCmd(
  args: MigrateInteractiveCmdArgs | string | string[],
  preparsed?: Partial<MigrateInteractiveCmdArgs>,
): Promise<void> {
  args = parseIfNeeded(migrateInteractiveCmdArgs, args, preparsed);
  const db = connectDb(args);

  return await interactiveMigration({
    db,
    name: args.name,
    outputArgs: args,
  });
}

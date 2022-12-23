import { Argv } from "yargs";
import { MigrateCmdArgs } from "../migrateCmd";
import { connectDb } from "../../auth/connectDb";
import { editMigration } from "../../migrations/editMigration";
import { MapFunction } from "../../views/viewDocument";

export const command = ["edit <name>", "add <name>"];
export const desc = "add or edit a migration";

export type MigrateEditCmdArgs = MigrateCmdArgs & {
  name: string;
  function: MapFunction | string;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("name", {
      describe: "name of the migration",
    })
    .options({
      function: {
        description:
          "map function string to use for the migration, rather than editing in terminal",
      },
    });
}

export async function migrateEditCmd(args: MigrateEditCmdArgs): Promise<void> {
  const db = await connectDb(args);

  return await editMigration({
    db,
    migrationName: args.name,
    mapFn: args.function,
    outputArgs: args,
  });
}

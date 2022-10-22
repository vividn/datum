import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import { migrateEditCmd, MigrateEditCmdArgs } from "./migrate/migrateEditCmd";

export const command = ["migrate", "migration", "mig"];
export const desc = "migrate data from one state to another";

export type MigrateCmdArgs = BaseDatumArgs;

export function builder(yargs: Argv): Argv {
  return yargs.commandDir("./migrate");
}

export async function migrateCmd(args: MigrateCmdArgs): Promise<unknown> {
  switch (args._?.[1]) {
    case "edit":
    case "add":
      return await migrateEditCmd(args as unknown as MigrateEditCmdArgs);

    default:
      throw Error("command not recognized");
  }
}

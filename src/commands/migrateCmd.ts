import { Argv } from "yargs";
import { migrateEditCmd, MigrateEditCmdArgs } from "./migrate/migrateEditCmd";
import { MainDatumArgs } from "../input/mainYargs";

export const command = ["migrate", "migration", "mig"];
export const desc = "migrate data from one state to another";

export type MigrateCmdArgs = MainDatumArgs;

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

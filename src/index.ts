#!/usr/bin/env node
import { baseYargs } from "./input/baseYargs";
import { DocExistsError } from "./documentControl/base";
import { addCmd, AddCmdArgs } from "./commands/addCmd";
import { mapCmd, MapCmdArgs } from "./commands/mapCmd";
import { setupCmd, SetupCmdArgs } from "./commands/setupCmd";
import { deleteCmd, DeleteCmdArgs } from "./commands/deleteCmd";
import { updateCmd, UpdateCmdArgs } from "./commands/updateCmd";
import { getCmd, GetCmdArgs } from "./commands/getCmd";
import { tailCmd, TailCmdArgs } from "./commands/tailCmd";
import { editCmd, EditCmdArgs } from "./commands/editCmd";
import { Show } from "./output/output";
import { v1Cmd, V1CmdArgs } from "./commands/v1Cmd";
import { migrateCmd, MigrateCmdArgs } from "./commands/migrateCmd";

export async function main(cliInput: string | string[]): Promise<void> {
  const args = await baseYargs.parse(cliInput);
  switch (args._[0]) {
    case "add":
      await addCmd(args as unknown as AddCmdArgs);
      break;

    case "get":
    case "see":
      await getCmd(args as unknown as GetCmdArgs);
      break;

    case "update": {
      const updateArgs = args as unknown as UpdateCmdArgs;
      updateArgs.strategy = updateArgs.strategy ?? "preferNew";
      await updateCmd(updateArgs);
      break;
    }

    case "merge": {
      const updateArgs = args as unknown as UpdateCmdArgs;
      updateArgs.strategy = updateArgs.strategy ?? "merge";
      await updateCmd(updateArgs);
      break;
    }

    case "delete":
    case "del":
      await deleteCmd(args as unknown as DeleteCmdArgs);
      break;

    case "map":
      await mapCmd(args as unknown as MapCmdArgs);
      break;

    case "setup": {
      const setupArgs = args as unknown as SetupCmdArgs;
      setupArgs.projectDir ??= process.env["HOME"] + "/.projectDatumViews";
      setupArgs.show =
        setupArgs.show === Show.Default ? Show.Minimal : setupArgs.show;
      await setupCmd(setupArgs);
      break;
    }

    case "tail":
      await tailCmd(args as unknown as TailCmdArgs);
      break;

    case "edit":
      await editCmd(args as unknown as EditCmdArgs);
      break;

    case "v1":
      await v1Cmd(args as unknown as V1CmdArgs);
      break;

    case "migrate":
    case "migration":
    case "mig":
      await migrateCmd(args as unknown as MigrateCmdArgs);
      break;

    default:
      throw Error("command not recognized");
  }
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    if (err instanceof DocExistsError) {
      process.exitCode = 11;
    } else {
      console.error(err);
    }
  });
}

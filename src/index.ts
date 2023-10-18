#!/usr/bin/env node
import { DocExistsError } from "./documentControl/base";
import { addCmd, AddCmdArgs } from "./commands/addCmd";
import { mapCmd, MapCmdArgs } from "./commands/mapCmd";
import { setupCmd, SetupCmdArgs } from "./commands/setupCmd";
import { deleteCmd, DeleteCmdArgs } from "./commands/deleteCmd";
import { updateCmd, UpdateCmdArgs } from "./commands/updateCmd";
import { getCmd, GetCmdArgs } from "./commands/getCmd";
import { tailCmd, TailCmdArgs } from "./commands/tailCmd";
import { editCmd, EditCmdArgs } from "./commands/editCmd";
import { v1Cmd, V1CmdArgs } from "./commands/v1Cmd";
import { migrateCmd, MigrateCmdArgs } from "./commands/migrateCmd";
import { reduceCmd, ReduceCmdArgs } from "./commands/reduceCmd";
import { Show } from "./input/outputArgs";
import { MainDatumArgs, mainYargs } from "./input/mainYargs";
import { grepCmd, GrepCmdArgs } from "./commands/grepCmd";
import { occurCmd, OccurCmdArgs } from "./commands/occurCmd";
import { startCmd, StartCmdArgs } from "./commands/startCmd";
import { switchCmd, SwitchCmdArgs } from "./commands/switchCmd";
import { endCmd, EndCmdArgs } from "./commands/endCmd";

export async function main(cliInput: string | string[]): Promise<void> {
  const args = (await mainYargs.parse(cliInput)) as MainDatumArgs;
  switch (args._?.[0]) {
    case "add":
      await addCmd(args as unknown as AddCmdArgs);
      break;

    case "occur":
      await occurCmd(args as unknown as OccurCmdArgs);
      break;

    case "start":
      await startCmd(args as unknown as StartCmdArgs);
      break;

    case "end":
      await endCmd(args as unknown as EndCmdArgs);
      break;

    case "switch":
      await switchCmd(args as unknown as SwitchCmdArgs);
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

    case "reduce":
    case "red":
      await reduceCmd(args as unknown as ReduceCmdArgs);
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

    case "head": {
      const headArgs = args as unknown as TailCmdArgs;
      headArgs.head = true;
      await tailCmd(headArgs);
      break;
    }

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

    case "grep":
      await grepCmd(args as unknown as GrepCmdArgs);
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

#!/usr/bin/env node
import { DocExistsError } from "./documentControl/base";
import { mainArgs } from "./input/mainArgs";
import { parse as shellParse } from "shell-quote";

export async function main(cliInput: string | string[]): Promise<void> {
  const args =
    typeof cliInput === "string"
      ? (shellParse(cliInput) as string[])
      : cliInput;
  const parsed = mainArgs.parse_args(args);
  console.log({ parsed });
  process.exit(0);
  //
  // switch (args._?.[0]) {
  //   case "add":
  //     await addCmd(args as unknown as AddCmdArgs);
  //     break;
  //
  //   case "occur":
  //     await occurCmd(args as unknown as OccurCmdArgs);
  //     break;
  //
  //   case "start":
  //     await startCmd(args as unknown as StartCmdArgs);
  //     break;
  //
  //   case "end":
  //     await endCmd(args as unknown as EndCmdArgs);
  //     break;
  //
  //   case "switch":
  //     await switchCmd(args as unknown as SwitchCmdArgs);
  //     break;
  //
  //   case "get":
  //   case "see":
  //     await getCmd(args as unknown as GetCmdArgs);
  //     break;
  //
  //   case "update": {
  //     const updateArgs = args as unknown as UpdateCmdArgs;
  //     updateArgs.strategy = updateArgs.strategy ?? "preferNew";
  //     await updateCmd(updateArgs);
  //     break;
  //   }
  //
  //   case "merge": {
  //     const updateArgs = args as unknown as UpdateCmdArgs;
  //     updateArgs.strategy = updateArgs.strategy ?? "merge";
  //     await updateCmd(updateArgs);
  //     break;
  //   }
  //
  //   case "delete":
  //   case "del":
  //     await deleteCmd(args as unknown as DeleteCmdArgs);
  //     break;
  //
  //   case "map":
  //     await mapCmd(args as unknown as MapCmdArgs);
  //     break;
  //
  //   case "reduce":
  //   case "red":
  //     await reduceCmd(args as unknown as ReduceCmdArgs);
  //     break;
  //
  //   case "setup": {
  //     const setupArgs = args as unknown as SetupCmdArgs;
  //     setupArgs.projectDir ??= process.env["HOME"] + "/.projectDatumViews";
  //     setupArgs.show =
  //       setupArgs.show === Show.Default ? Show.Minimal : setupArgs.show;
  //     await setupCmd(setupArgs);
  //     break;
  //   }
  //
  //   case "tail":
  //     await tailCmd(args as unknown as TailCmdArgs);
  //     break;
  //
  //   case "head": {
  //     await headCmd(args as unknown as HeadCmdArgs);
  //     break;
  //   }
  //
  //   case "edit":
  //     await editCmd(args as unknown as EditCmdArgs);
  //     break;
  //
  //   case "v1":
  //     await v1Cmd(args as unknown as V1CmdArgs);
  //     break;
  //
  //   case "migrate":
  //   case "migration":
  //   case "mig":
  //     await migrateCmd(args as unknown as MigrateCmdArgs);
  //     break;
  //
  //   case "grep":
  //     await grepCmd(args as unknown as GrepCmdArgs);
  //     break;
  //
  //   case "backup":
  //     await backupCmd(args as unknown as BackupCmdArgs);
  //     break;
  //
  //   case "restore":
  //     await restoreCmd(args as unknown as RestoreCmdArgs);
  //     break;
  //
  //   default:
  //     throw Error("command not recognized");
  // }
}

if (require.main === module) {
  if (["", undefined].includes(process.env["NODE_ENV"])) {
    process.env.NODE_ENV = "production";
  }
  main(process.argv.slice(2)).catch((err) => {
    if (err instanceof DocExistsError) {
      process.exit(11);
    } else {
      if (process.env["NODE_ENV"] === "production") {
        console.error(`${err.name ?? "Error"}:`, err.message);
      } else {
        console.error(err);
      }
      process.exit(1);
    }
  });
}

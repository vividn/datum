#!/usr/bin/env node
import { DocExistsError } from "./documentControl/base";
import { parse as shellParse } from "shell-quote";
import { ArgumentParser } from "argparse";
import { addCmd } from "./commands/addCmd";
import { dbArgs } from "./input/dbArgs";
import { occurCmd } from "./commands/occurCmd";
import { startCmd } from "./commands/startCmd";
import { endCmd } from "./commands/endCmd";
import { switchCmd } from "./commands/switchCmd";
import { getCmd } from "./commands/getCmd";
import { updateCmd } from "./commands/updateCmd";

const commandParser = new ArgumentParser({
  description:
    "Initially just parse the first positional argument as a command to forward for additional parsing",
  add_help: false,
  parents: [dbArgs],
});
commandParser.add_argument("command", {
  help: "the command to run",
});

export async function datum(cliInput: string | string[]): Promise<void> {
  const cliArgs =
    typeof cliInput === "string"
      ? (shellParse(cliInput) as string[])
      : cliInput;
  const [namespace, args] = commandParser.parse_known_args(cliArgs);
  const command = namespace.command;
  switch (command) {
    case "add":
      await addCmd(args, namespace);
      break;

    case "occur":
      await occurCmd(args, namespace);
      break;

    case "start":
      await startCmd(args, namespace);
      break;

    case "end":
      await endCmd(args, namespace);
      break;

    case "switch":
      await switchCmd(args, namespace);
      break;

    case "get":
      await getCmd(args, namespace);
      break;

    case "update": {
      namespace.strategy ??= "preferNew";
      await updateCmd(args, namespace);
      break;
    }

    case "merge": {
      namespace.strategy ??= "merge";
      await updateCmd(args, namespace);
      break;
    }
    //
    //   case "delete":
    //   case "del":
    //     await deleteCmd(remaining_args, namespace);
    //     break;
    //
    //   case "map":
    //     await mapCmd(remaining_args, namespace);
    //     break;
    //
    //   case "reduce":
    //   case "red":
    //     await reduceCmd(remaining_args, namespace);
    //     break;
    //
    //   case "setup": {
    //     const setupArgs = args as unknown as SetupCmdArgs;
    //     setupArgs.projectDir ??= process.env["HOME"] + "/.projectDatumViews";
    //     setupArgs.show =
    //       setupArgs.show === Show.Default ? Show.Minimal : setupArgs.show;
    //     await setupCmd(remaining_args, namespace);
    //     break;
    //   }
    //
    //   case "tail":
    //     await tailCmd(remaining_args, namespace);
    //     break;
    //
    //   case "head": {
    //     await headCmd(remaining_args, namespace);
    //     break;
    //   }
    //
    //   case "edit":
    //     await editCmd(remaining_args, namespace);
    //     break;
    //
    //   case "v1":
    //     await v1Cmd(remaining_args, namespace);
    //     break;
    //
    //   case "migrate":
    //   case "migration":
    //   case "mig":
    //     await migrateCmd(remaining_args, namespace);
    //     break;
    //
    //   case "grep":
    //     await grepCmd(remaining_args, namespace);
    //     break;
    //
    //   case "backup":
    //     await backupCmd(remaining_args, namespace);
    //     break;
    //
    //   case "restore":
    //     await restoreCmd(remaining_args, namespace);
    //     break;
    //
    default:
      throw Error(`command "${command}" not recognized`);
  }
}

if (require.main === module) {
  if (["", undefined].includes(process.env["NODE_ENV"])) {
    process.env.NODE_ENV = "production";
  }
  datum(process.argv.slice(2)).catch((err) => {
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

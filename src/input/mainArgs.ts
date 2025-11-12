import { parse as shellParse } from "shell-quote";
import { ArgumentParser } from "argparse";
import { dbArgs, DbArgs } from "./dbArgs.js";
import { OutputArgs, Show } from "./outputArgs.js";
import { addCmd } from "../commands/addCmd.js";
import { occurCmd } from "../commands/occurCmd.js";
import { startCmd } from "../commands/startCmd.js";
import { endCmd } from "../commands/endCmd.js";
import { switchCmd } from "../commands/switchCmd.js";
import { getCmd } from "../commands/getCmd.js";
import { updateCmd } from "../commands/updateCmd.js";
import { deleteCmd } from "../commands/deleteCmd.js";
import { mapCmd } from "../commands/mapCmd.js";
import { reduceCmd } from "../commands/reduceCmd.js";
import { setupCmd } from "../commands/setupCmd.js";
import { tailCmd } from "../commands/tailCmd.js";
import { headCmd } from "../commands/headCmd.js";
import { editCmd } from "../commands/editCmd.js";
import { v1Cmd } from "../commands/v1Cmd.js";
import { grepCmd } from "../commands/grepCmd.js";
import { backupCmd } from "../commands/backupCmd.js";
import { restoreCmd } from "../commands/restoreCmd.js";
import { migrateCmd } from "../commands/migrateCmd.js";
import { checkCmd } from "../commands/checkCmd.js";
import { dayviewCmd } from "../commands/dayviewCmd.js";
import { nowviewCmd } from "../commands/nowviewCmd.js";
import { syncCmd } from "../commands/syncCmd.js";
import { retimeCmd } from "../commands/retimeCmd.js";
import { topLevelHelpParser } from "./topLevelHelp.js";

export type MainDatumArgs = DbArgs & OutputArgs;

const commandParser = new ArgumentParser({
  prog: "datum",
  description: "track data on your life",
  add_help: false,
  parents: [dbArgs],
});

commandParser.add_argument("command", {
  help: "the command to run",
  nargs: "?",
});

export async function datum(cliInput: string | string[]): Promise<void> {
  const cliArgs =
    typeof cliInput === "string"
      ? (shellParse(cliInput) as string[])
      : cliInput;

  const [namespace, args] = commandParser.parse_known_args(cliArgs);
  // When calling from the command line, SHOW should be set to default
  namespace.show = Show.Default;

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
    case "stop":
      await endCmd(args, namespace);
      break;

    case "switch":
      await switchCmd(args, namespace);
      break;

    case "get":
      await getCmd(args, namespace);
      break;

    case "update": {
      namespace.strategy ??= "update";
      await updateCmd(args, namespace);
      break;
    }

    case "merge": {
      namespace.strategy ??= "merge";
      await updateCmd(args, namespace);
      break;
    }

    case "rekey": {
      namespace.strategy ??= "rekey";
      await updateCmd(args, namespace);
      break;
    }

    case "delete":
    case "del":
      await deleteCmd(args, namespace);
      break;

    case "map":
      await mapCmd(args, namespace);
      break;

    case "reduce":
    case "red":
      await reduceCmd(args, namespace);
      break;

    case "setup": {
      namespace.show =
        namespace.show === Show.Default ? Show.Minimal : namespace.show;
      await setupCmd(args, namespace);
      break;
    }

    case "tail":
      await tailCmd(args, namespace);
      break;

    case "head": {
      await headCmd(args, namespace);
      break;
    }

    case "edit":
      await editCmd(args, namespace);
      break;

    case "v1":
      await v1Cmd(args, namespace);
      break;

    case "grep":
      await grepCmd(args, namespace);
      break;

    case "backup":
      await backupCmd(args, namespace);
      break;

    case "restore":
      await restoreCmd(args, namespace);
      break;

    case "migrate":
    case "migration":
    case "mig":
      await migrateCmd(args, namespace);
      break;

    case "check": {
      const errors = await checkCmd(args, namespace);
      if (!errors.ok) {
        process.exit(1);
      }
      break;
    }

    case "dayview":
      await dayviewCmd(args, namespace);
      break;

    case "nowview":
      await nowviewCmd(args, namespace);
      break;
    // case "test": {
    //   const parser = new ArgumentParser({});
    //   parser.add_argument("--test", {
    //     help: "test",
    //     action: BooleanOptionalAction,
    //   });
    //   const ns = parser.parse_intermixed_args(args, namespace);
    //   break;
    // }

    case "sync":
      await syncCmd(args, namespace);
      break;

    case "retime":
    case "rt":
      await retimeCmd(args, namespace);
      break;

    case "help":
    case undefined:
      topLevelHelpParser.print_help();
      break;

    default: {
      topLevelHelpParser.print_help();
      throw Error(`command "${command}" not recognized`);
    }
  }
}

export const mainArgs = commandParser;

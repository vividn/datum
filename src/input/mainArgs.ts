import { parse as shellParse } from "shell-quote";
import { ArgumentParser, RawDescriptionHelpFormatter } from "argparse";
import { dbArgs, DbArgs } from "./dbArgs";
import { OutputArgs, Show } from "./outputArgs";
import { addCmd } from "../commands/addCmd";
import { occurCmd } from "../commands/occurCmd";
import { startCmd } from "../commands/startCmd";
import { endCmd } from "../commands/endCmd";
import { switchCmd } from "../commands/switchCmd";
import { getCmd } from "../commands/getCmd";
import { updateCmd } from "../commands/updateCmd";
import { deleteCmd } from "../commands/deleteCmd";
import { mapCmd } from "../commands/mapCmd";
import { reduceCmd } from "../commands/reduceCmd";
import { setupCmd } from "../commands/setupCmd";
import { tailCmd } from "../commands/tailCmd";
import { headCmd } from "../commands/headCmd";
import { editCmd } from "../commands/editCmd";
import { v1Cmd } from "../commands/v1Cmd";
import { grepCmd } from "../commands/grepCmd";
import { backupCmd } from "../commands/backupCmd";
import { restoreCmd } from "../commands/restoreCmd";
import { migrateCmd } from "../commands/migrateCmd";
import { checkCmd } from "../commands/checkCmd";
import { dayviewCmd } from "../commands/dayviewCmd";
import { syncCmd } from "../commands/syncCmd";
import { retimeCmd } from "../commands/retimeCmd";

export type MainDatumArgs = DbArgs & OutputArgs;

const commandParser = new ArgumentParser({
  prog: "datum",
  description: "track data on your life",
  add_help: true,
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

  if (cliArgs.length === 0 || cliArgs[0] === "-h" || cliArgs[0] === "--help") {
    const helpParser = new ArgumentParser({
      prog: "datum",
      description: `
          Track data on your life.

          Usage:
            datum <command> [options]

          Commands:
            add         Add a new document to the database.
            occur       Record the occurrence of an event at a specific time.
            start       Record the start of an event that occurs in blocks of time.
            end, stop   Record the end of an event.
            switch      Switch the state of a given field.
            get         Retrieve and display a document by its ID.
            update      Update an existing document with new data.
            merge       Merge new data into an existing document.
            rekey       Change the key of an existing document.
            delete, del Delete a document from the database.
            map         Display a map view or map-reduce view of the data.
            reduce, red Perform a reduction on a map view.
            setup       Set up the database for use with datum.
            tail        Show the most recent entries in the database.
            head        Show the earliest entries in the database.
            edit        Edit a document directly using your editor.
            v1          Export data in the old datum format.
            grep        Search for documents matching a pattern.
            backup      Create a backup of the database.
            restore     Restore the database from a backup file.
            migrate     Migrate data from one state to another.
            check       Check for problems in the data and optionally fix them.
            dayview     Generate a visual representation of data for a day.
            sync        Synchronize the database with a remote host.
            retime      Change the time of an existing document.
            
          Options:
            --db, --database <name>    Specify the database to use (default: "datum").
            --host <host>              Specify the host and port (default: "localhost:5984").
            --username <user>          Specify the username for authentication.
            --password <password>      Specify the password for authentication.
            --env <file>               Load environment variables from a file.
            --config <file>            Use a custom configuration file.
            --create-db                Create the database if it does not exist.
            --show-all, -A             Show complete documents in output.
            --show <level>             Specify the level of detail in output (e.g., "minimal", "standard").
            --format-string, -o <fmt>  Use a custom format string for output.`,
      epilog: `Run 'datum <command> --help' for more information about a specific command.`,
      add_help: false,
      formatter_class: RawDescriptionHelpFormatter,
    });
    helpParser.print_help();
    return;
  }

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

    default:
      throw Error(`command "${command}" not recognized`);
  }
}

export const mainArgs = commandParser;

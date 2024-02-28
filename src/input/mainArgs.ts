import { parse as shellParse } from "shell-quote";
import { ArgumentParser } from "argparse";
import { dbArgs, DbArgs } from "./dbArgs";
import { OutputArgs, Show } from "./outputArgs";
import { dtmAdd } from "../commands/dtmAdd";
import { dtmOccur } from "../commands/dtmOccur";
import { dtmStart } from "../commands/dtmStart";
import { dtmEnd } from "../commands/dtmEnd";
import { dtmSwitch } from "../commands/dtmSwitch";
import { dtmGet } from "../commands/dtmGet";
import { dtmUpdate } from "../commands/dtmUpdate";
import { dtmDelete } from "../commands/dtmDelete";
import { dtmMap } from "../commands/dtmMap";
import { dtmReduce } from "../commands/dtmReduce";
import { dtmSetup } from "../commands/dtmSetup";
import { dtmTail } from "../commands/dtmTail";
import { dtmHead } from "../commands/dtmHead";
import { dtmEdit } from "../commands/dtmEdit";
import { dtmV1 } from "../commands/dtmV1";
import { dtmGrep } from "../commands/dtmGrep";
import { dtmBackup } from "../commands/dtmBackup";
import { dtmRestore } from "../commands/dtmRestore";
import { dtmMigrate } from "../commands/dtmMigrate";

export type MainDatumArgs = DbArgs & OutputArgs;

const commandParser = new ArgumentParser({
  prog: "datum",
  description: "track data on your life",
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
      await dtmStart(args, namespace);
      break;

    case "end":
      await dtmEnd(args, namespace);
      break;

    case "switch":
      await dtmSwitch(args, namespace);
      break;

    case "get":
      await dtmGet(args, namespace);
      break;

    case "update": {
      namespace.strategy ??= "preferNew";
      await dtmUpdate(args, namespace);
      break;
    }

    case "merge": {
      namespace.strategy ??= "merge";
      await dtmUpdate(args, namespace);
      break;
    }

    case "delete":
    case "del":
      await dtmDelete(args, namespace);
      break;

    case "map":
      await dtmMap(args, namespace);
      break;

    case "reduce":
    case "red":
      await dtmReduce(args, namespace);
      break;

    case "setup": {
      namespace.projectDir ??= process.env["HOME"] + "/.projectDatumViews";
      namespace.show =
        namespace.show === Show.Default ? Show.Minimal : namespace.show;
      await dtmSetup(args, namespace);
      break;
    }

    case "tail":
      await dtmTail(args, namespace);
      break;

    case "head": {
      await dtmHead(args, namespace);
      break;
    }

    case "edit":
      await dtmEdit(args, namespace);
      break;

    case "v1":
      await dtmV1(args, namespace);
      break;

    case "grep":
      await dtmGrep(args, namespace);
      break;

    case "backup":
      await dtmBackup(args, namespace);
      break;

    case "restore":
      await dtmRestore(args, namespace);
      break;

    case "migrate":
    case "migration":
    case "mig":
      await dtmMigrate(args, namespace);
      break;

    // case "test": {
    //   const parser = new ArgumentParser({});
    //   parser.add_argument("--test", {
    //     help: "test",
    //     action: BooleanOptionalAction,
    //   });
    //   const ns = parser.parse_intermixed_args(args, namespace);
    //   console.debug({ ns });
    //   break;
    // }

    default:
      throw Error(`command "${command}" not recognized`);
  }
}

export const mainArgs = commandParser;

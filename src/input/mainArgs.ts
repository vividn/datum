import { ArgumentParser } from "argparse";
import { dbArgs, DbArgs } from "./dbArgs";
import { OutputArgs } from "./outputArgs";

export type MainDatumArgs = {
  _?: (string | number)[];
} & DbArgs &
  OutputArgs;

const argparser = new ArgumentParser({
  prog: "datum",
  description: "track data on your life",
  add_help: false,
  parents: [dbArgs],
});
argparser.add_argument("command", {
  help: "the command to run",
});

export async function datum(cliInput: string | string[]): Promise<void> {
  const args =
    typeof cliInput === "string" ? (cliInput.split(" ") as string[]) : cliInput;
  const [namespace, remaining_args] = argparser.parse_known_args(args);
  const command = namespace.command;

  console.log({ namespace, remaining_args });
  process.exit(0);
}
// const subparsers = argparser.add_subparsers({ title: "commands" });
// addCmdParser(subparsers);

export const mainArgs = argparser;

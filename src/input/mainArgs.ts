import { ArgumentParser } from "argparse";
import { DbArgs, dbArgs } from "./dbArgs";
import { outputArgs, OutputArgs } from "./outputArgs";

export type MainDatumArgs = {
  _?: (string | number)[];
} & DbArgs &
  OutputArgs;

const argparser = new ArgumentParser({
  prog: "datum",
  description: "track data on your life",
  parents: [dbArgs, outputArgs],
  add_help: false,
});
argparser.add_argument("command", {
  help: "the command to run",
});


// const subparsers = argparser.add_subparsers({ title: "commands" });
// addCmdParser(subparsers);

export const mainArgs = argparser;

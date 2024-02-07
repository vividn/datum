import { ArgumentParser } from "argparse";
import { addCmdParser } from "../commands/addCmd";
import { DbArgs, dbArgs } from "./dbArgs";
import { outputArgs, OutputArgs } from "./outputArgs";

export type MainDatumArgs = {
  _?: (string | number)[];
} & DbArgs &
  OutputArgs;

const argparser = new ArgumentParser({
  prog: "datum",
  description: "track data on your life",
});

const subparsers = argparser.add_subparsers({ title: "commands" });
addCmdParser(subparsers);

export const mainArgs = outputArgs(dbArgs(argparser));

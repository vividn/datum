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
});
export const mainArgs = outputArgs(dbArgs(argparser));

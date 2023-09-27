import { BaseArgs, baseArgs } from "./baseArgs";
import { OutputArgs } from "./outputArgs";
import yargs, { Argv } from "yargs";

export type MainDatumArgs = {
  _?: (string | number)[];
} & BaseArgs &
  OutputArgs;

export function mainYargs(otherYargs?: Argv): Argv {
  const yarg = otherYargs ?? yargs;
  return baseArgs(yarg).commandDir("../commands").help("h").alias("h", "help");
}

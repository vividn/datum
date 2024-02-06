import { DbArgs, dbArgs } from "./dbArgs";
import { OutputArgs } from "./outputArgs";

export type MainDatumArgs = {
  _?: (string | number)[];
} & DbArgs &
  OutputArgs;

export const mainYargs = baseArgs
  .commandDir("../commands")
  .help("h")
  .alias("h", "help");

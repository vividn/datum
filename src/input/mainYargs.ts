import { BaseArgs, baseArgs } from "./baseArgs";
import { OutputArgs } from "./outputArgs";

export type MainDatumArgs = {
  _?: string[];
} & BaseArgs &
  OutputArgs;

export const mainYargs = baseArgs
  .commandDir("../commands")
  .help("h")
  .alias("h", "help");

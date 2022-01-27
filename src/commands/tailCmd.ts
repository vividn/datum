import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";

export const command = ["tail [field]"];
export const desc =
  "show the most recently occured/modified/created entries in the db";

export type TailCmdArgs = BaseDatumArgs & {
  num?: number;
  field?: string;
  metric?: "occur" | "create" | "modify";
  format?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.options({
    num: {
      alias: ["n", "number"],
      describe: "number of entries to show, defaults to 10",
      type: "number",
    },
    field: {
      describe: "limit entries to a particular field of data",
      alias: "f",
      nargs: 1,
      type: "string",
    },
    metric: {
      describe: "which time to use for the sorting",
      choices: ["occur", "create", "modify"],
      alias: "m",
      type: "string",
    },
    format: {
      describe: "custom format for outputting the data",
      type: "string",
    },
  });
}

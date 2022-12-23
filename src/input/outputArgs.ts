import yargs, { Argv } from "yargs";

export type OutputArgs = {
  showAll?: boolean;
  show?: Show;
  formatString?: string;
};

export enum Show {
  Default = "default",
  None = "none",
  Minimal = "minimal",
  Standard = "standard",
  Format = "format",
  All = "all",
}

export function outputYargs(otherYargs?: Argv): Argv {
  const yarg = otherYargs ?? yargs;
  return yarg.group(["show", "showAll", "formatString"], "Output").options({
    "show-all": {
      describe: "Show complete document when displaying, not just data",
      type: "boolean",
      alias: "A",
    },
    show: {
      describe: "how much of documents to show",
      type: "string",
      choices: Object.values(Show),
      default: "default",
      conflict: "show-all",
    },
    "format-string": {
      describe:
        "create a custom output string for visualizing the doc(s). Specify %keys% with percent signs",
      type: "string",
      alias: "o",
    },
  });
}

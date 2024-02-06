import { ArgumentParser } from "argparse";

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
export function outputArgs(parser: ArgumentParser): ArgumentParser {
  parser.add_argument("--show-all", {
    help: "Show complete document when displaying, not just data",
    action: "store_true",
    dest: "showAll",
  });
  parser.add_argument("--show", {
    help: "how much of documents to show",
    choices: Object.values(Show),
    default: Show.Default,
  });
  parser.add_argument("--format-string", {
    help: "create a custom output string for visualizing the doc(s). Specify %keys% with percent signs",
    dest: "formatString",
  });
  return parser;
}

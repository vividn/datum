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

export const outputArgs = new ArgumentParser({
  add_help: false,
});
const outputGroup = outputArgs.add_argument_group({
  title: "Output",
  description: "Options for display on the terminal",
});
outputGroup.add_argument("--show-all", {
  help: "Show complete document when displaying, not just data",
  action: "store_true",
  dest: "showAll",
});
outputGroup.add_argument("--show", {
  help: "how much of documents to show",
  choices: Object.values(Show),
});
outputGroup.add_argument("--format-string", {
  help: "create a custom output string for visualizing the doc(s). Specify %%keys%% with percent signs",
  dest: "formatString",
});

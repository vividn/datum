import { ArgumentParser } from "argparse";

export type FieldArgs = {
  field?: string;
  fieldless?: boolean;
};

export function fieldArgs(parser: ArgumentParser): ArgumentParser {
  const fieldGroup = parser.add_argument_group({
    title: "Field",
    description: "Options for selecting a field",
  });
  fieldGroup.add_argument("field", {
    help: "what is being tracked",
    type: "string",
  });
  fieldGroup.add_argument("--fieldless", {
    help: "do not include field as a positional argument",
    action: "store_true",
    dest: "fieldless",
  });
  return parser;
}

import { ArgumentParser } from "argparse";

export type FieldArgs = {
  field?: string;
  fieldless?: boolean;
};

const fieldArgs = new ArgumentParser({
  add_help: false,
});
const fieldGroup = fieldArgs.add_argument_group({
  title: "Field",
  description: "Options for selecting a field",
});
fieldGroup.add_argument("field", {
  help: "what is being tracked; can use %fieldName% syntax to compose from other data",
  nargs: "?",
});
fieldGroup.add_argument("--fieldless", "-F", {
  help: "do not include field as a positional argument",
  action: "store_true",
  dest: "fieldless",
});
export { fieldArgs };

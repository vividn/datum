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
  help: "what is being tracked",
});
fieldGroup.add_argument("--fieldless", {
  help: "do not include field as a positional argument",
  action: "store_true",
  dest: "fieldless",
});
export { fieldArgs };

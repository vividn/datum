import yargs, { Argv } from "yargs";

export type FieldArgs = {
  field?: string;
  fieldless?: boolean;
};

export function fieldArgs(otherYargs?: Argv): Argv {
  const yarg = otherYargs ?? yargs;
  return yarg
    .positional("field", {
      // TODO Maybe update description of field to fit in more contexts
      describe: "what is being tracked",
      type: "string",
    })
    .options({
      fieldless: {
        describe: "do not include field as a positional argument",
        type: "boolean",
        alias: "F",
      },
    });
}

import yargs, { Argv } from "yargs";

export type FieldArgs = {
  field: string;
};

export function fieldArgs(otherYargs?: Argv): Argv {
  const yarg = otherYargs ?? yargs;
  return yarg
    .positional("field", {
      // TODO Maybe update description of field to fit in more contexts
      describe:
        "field specifying what is being tracked, used by default as partition for the data, but can be changed with --partition",
      type: "string",
    })
    .options({
      fieldless: {
        describe: "do not include field as the first required key",
        type: "boolean",
        alias: "F",
      },
    });
}

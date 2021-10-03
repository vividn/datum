import yargs, { Argv } from "yargs";

export type DataInputArgs = {
  data?: (string | number)[];
  baseData?: string;
  field?: string;
  comment?: string | string[];
  required?: string | string[];
  optional?: string | string[];
  remainder?: string;
  stringRemainder?: boolean;
  lenient?: boolean;
};

export function dataYargs(otherYargs?: Argv): Argv {
  const yarg = otherYargs ?? yargs;
  return yarg
    .positional("data", {
      describe:
        "The data to put in the document. " +
        "Data must include one argument for each key specified by --required. " +
        "Once required keys are filled, data will be assigned to keys specified by --optional. " +
        'Additional data can be specified in a "key=data" format. ' +
        "Any data that does not have a key will be put in the key specified with --remainder, unless strict mode is on",
    })
    .group(
      [
        "base-data",
        "required",
        "optional",
        "remainder",
        "string-remainder",
        "lenient",
      ],
      "Keys & Data"
    )
    .options({
      "base-data": {
        describe:
          "base object on which additional keys are added. Fed through relaxed-json, but must still parse to an object. Use with --no-metadata for raw json input into couchdb. Default: {}",
        nargs: 1,
        alias: "b",
        type: "string",
      },
      field: {
        describe:
          "field specifying what is being tracked, used by default as partition for the data, but can be changed with --partition",
        alias: "f",
        nargs: 1,
        type: "string",
      },
      comment: {
        describe: "comment to include in the data",
        alias: "c",
        nargs: 1,
        type: "string",
      },
      required: {
        describe:
          "Add a required key to the data, will be filled with first keyless data. If not enough data is specified to fill all required keys, an error will be thrown",
        alias: ["K", "req"],
        type: "string",
        nargs: 1,
      },
      optional: {
        describe:
          "Add an optional key to the data, will be filled with first keyless data. A default value can be specified with an '=', e.g., -k key=value",
        alias: ["k", "opt"],
        type: "string",
        nargs: 1,
      },
      remainder: {
        describe:
          "Any extra data supplied will be put into this key as an array. When --lenient is specified, defaults to 'extraData'",
        alias: ["rem", "R"],
        type: "string",
        nargs: 1,
      },
      "string-remainder": {
        describe:
          "Remainder data will be a space-concatenated string rather than an array",
        alias: "S",
        type: "boolean",
      },
      lenient: {
        //TODO: Invert this to be strict
        describe: "Allow extra data without defined keys",
        type: "boolean",
        alias: "l",
      },
    });
}

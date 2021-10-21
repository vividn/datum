import yargs, { Argv } from "yargs";
import { DatumData } from "../documentControl/DatumDocument";
import inferType from "../utils/inferType";
import { BaseDataError, DataError } from "../errors";
import { splitFirst } from "../utils/splitFirst";
import { createOrAppend } from "../utils/createOrAppend";

export type DataArgs = {
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

export const handleDataArgs = function ({
  data = [],
  required = [],
  optional = [],
  remainder,
  stringRemainder,
  field,
  comment,
  lenient = false,
  baseData,
}: DataArgs): DatumData {
  const requiredKeys = typeof required === "string" ? [required] : required;
  const optionalKeys = typeof optional === "string" ? [optional] : optional;
  const remainderKey = remainder ?? (lenient ? "extraData" : undefined);
  const remainderData = [];

  const parsedData: DatumData = baseData ? inferType(baseData) : {};
  if (typeof parsedData !== "object" || parsedData === null) {
    throw new BaseDataError("base data not a valid object");
  }

  posArgsLoop: for (const arg of data) {
    const [beforeEquals, afterEquals] = splitFirst("=", String(arg));

    if (afterEquals !== undefined) {
      // explicit key is given e.g., 'key=value'
      parsedData[beforeEquals] = inferType(afterEquals);
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = beforeEquals;

    requiredKeysLoop: while (requiredKeys.length > 0) {
      const dataKey = requiredKeys.shift()!;

      if (dataKey in parsedData) {
        continue requiredKeysLoop;
      }

      parsedData[dataKey] = inferType(dataValue);
      continue posArgsLoop;
    }

    optionalKeysLoop: while (optionalKeys.length > 0) {
      const [dataKey] = splitFirst("=", optionalKeys.shift()!);

      if (dataKey in parsedData) {
        continue optionalKeysLoop;
      }

      parsedData[dataKey] = inferType(dataValue);
      continue posArgsLoop;
    }

    remainderData.push(dataValue);
  }

  if (remainderData.length > 0) {
    if (remainderKey === undefined) {
      throw new DataError(
        "some data do not have keys. Assign keys with equals signs, use required/optional keys, specify a key to use as --remainder, or use --lenient"
      );
    }

    if (stringRemainder) {
      parsedData[remainderKey] = createOrAppend(
        parsedData[remainderKey],
        remainderData.join(" ")
      );
    } else {
      for (const remainder of remainderData) {
        parsedData[remainderKey] = createOrAppend(
          parsedData[remainderKey],
          inferType(remainder)
        );
      }
    }
  }

  if (requiredKeys.length > 0) {
    throw new DataError(
      `No data given for the required key(s) '${requiredKeys}`
    );
  }

  // If extra keys are left assign default values
  while (optionalKeys.length > 0) {
    const [dataKey, defaultValue] = splitFirst("=", optionalKeys.shift()!);

    if (dataKey in parsedData || defaultValue === undefined) {
      continue;
    }

    parsedData[dataKey] = inferType(defaultValue);
  }

  // put in field, overwriting if necessary
  if (field) {
    parsedData.field = inferType(field);
  }

  if (comment) {
    const inferredComments = (
      Array.isArray(comment)
        ? comment.map((comm) => inferType(comm))
        : [inferType(comment)]
    ) as any[];
    parsedData.comment = inferredComments.reduce(
      (accumulator, current) => createOrAppend(accumulator, current),
      parsedData["comment"]
    );
  }

  return parsedData;
};
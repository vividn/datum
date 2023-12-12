import yargs, { Argv } from "yargs";
import { DatumData } from "../documentControl/DatumDocument";
import { inferType } from "../utils/inferType";
import { BaseDataError, DataError } from "../errors";
import { splitFirst } from "../utils/splitFirst";
import { createOrAppend } from "../utils/createOrAppend";
import isPlainObject from "lodash.isplainobject";

export type DataArgs = {
  data?: (string | number)[];
  baseData?: string | DatumData;
  comment?: string | string[];
  required?: string | string[];
  optional?: string | string[];
  remainder?: string;
  stringRemainder?: boolean;
  commentRemainder?: boolean;
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
      "Keys & Data",
    )
    .options({
      "base-data": {
        describe:
          "base object on which additional keys are added. Fed through relaxed-json, but must still parse to an object. Use with --no-metadata for raw json input into couchdb. Default: {}",
        nargs: 1,
        alias: "b",
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
          "Add a required key to the data, will be filled with first keyless data. If not enough data is specified to fill all required keys, an error will be thrown.",
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
      "comment-remainder": {
        describe:
          "All unused data will be joined into a string and stored as a comment. Equivalent to `-SR comment`",
        alias: "C",
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

function isParsedBaseData(baseData: DatumData | string): baseData is DatumData {
  return isPlainObject(baseData);
}

export function parseBaseData(baseData?: DatumData | string): DatumData {
  const parsedData: DatumData = baseData
    ? isParsedBaseData(baseData)
      ? baseData
      : inferType(baseData)
    : {};
  if (typeof parsedData !== "object" || parsedData === null) {
    throw new BaseDataError("base data not a valid object");
  }
  return parsedData;
}

export function handleDataArgs(args: DataArgs): DatumData {
  args.data ??= [];
  args.required ??= [];
  args.optional ??= [];
  const {
    data,
    required,
    optional,
    remainder,
    stringRemainder,
    comment,
    lenient,
    baseData,
    commentRemainder,
  } = args;

  const requiredKeys = typeof required === "string" ? [required] : required;
  const optionalKeys = typeof optional === "string" ? [optional] : optional;

  const remainderKey =
    remainder ??
    (commentRemainder ? "comment" : lenient ? "extraData" : undefined);
  const remainderAsString = stringRemainder ?? commentRemainder;
  const remainderData = [];

  const parsedData = parseBaseData(baseData);
  // for idempotence of processing dataArgs, even when baseData is originally a string
  args.baseData = parsedData;

  posArgsLoop: while (data.length > 0) {
    const arg = data.shift()!;
    const [beforeEquals, afterEquals] = splitFirst("=", String(arg));

    if (afterEquals !== undefined) {
      // key is explicitly given e.g., 'key=value'
      const key = beforeEquals;
      const value = afterEquals;

      // Search for default value to allow explicitly setting it to default using '.'
      const existingValue = parsedData[key];
      const defaultValue =
        existingValue ??
        optionalKeys
          ?.find((optionalWithDefault) =>
            new RegExp(`^${key}=(.*)$`).test(optionalWithDefault),
          )
          ?.split("=")[1];
      parsedData[key] = inferType(value, key, defaultValue);
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = beforeEquals;

    requiredKeysLoop: while (requiredKeys.length > 0) {
      const dataKey = requiredKeys.shift()!;

      if (dataKey in parsedData) {
        continue requiredKeysLoop;
      }

      parsedData[dataKey] = inferType(dataValue, dataKey);
      continue posArgsLoop;
    }

    optionalKeysLoop: while (optionalKeys.length > 0) {
      const [dataKey, defaultValue] = splitFirst("=", optionalKeys.shift()!);

      if (dataKey in parsedData) {
        continue optionalKeysLoop;
      }

      parsedData[dataKey] = inferType(dataValue, dataKey, defaultValue);
      continue posArgsLoop;
    }

    remainderData.push(dataValue);
  }

  while (requiredKeys.length > 0) {
    const requiredKey = requiredKeys.shift()!;
    if (requiredKey in parsedData) {
      continue;
    }
    // Allow required keys to be given a default value via an optional key
    // This is useful for nested aliases where a key is required in the parent,
    // but then a child creates an optional default value for it
    // e.g. alias tx='datum occur tx -K acc -K amount'
    //      alias rent='tx acc=Checking -k amount=1200'
    // 'rent' would create a tx doc with amount=1200, while 'rent 1500' would create
    // a tx doc with amount=1500
    if (
      optionalKeys?.find((optionalWithDefault) =>
        new RegExp(`^${requiredKey}=`).test(optionalWithDefault),
      )
    ) {
      continue;
    }
    throw new DataError(`No data given for the required key: ${requiredKey}`);
  }

  // If optional keys with default values are left assign them
  while (optionalKeys.length > 0) {
    const [dataKey, defaultValue] = splitFirst("=", optionalKeys.shift()!);

    if (
      defaultValue === undefined ||
      (dataKey in parsedData && parsedData[dataKey] !== ".")
    ) {
      continue;
    }

    parsedData[dataKey] = inferType(defaultValue, dataKey);
  }

  if (comment) {
    const inferredComments = (
      Array.isArray(comment)
        ? comment.map((comm) => inferType(comm, "comment"))
        : [inferType(comment, "comment")]
    ) as any[];
    parsedData.comment = inferredComments.reduce(
      (accumulator, current) => createOrAppend(accumulator, current),
      parsedData["comment"],
    );
    delete args.comment;
  }

  if (remainderData.length > 0) {
    if (remainderKey === undefined) {
      throw new DataError(
        "some data do not have keys. Assign keys with equals signs, use required/optional keys, specify a key to use as --remainder, or use --lenient",
      );
    }

    if (remainderAsString) {
      parsedData[remainderKey] = createOrAppend(
        parsedData[remainderKey],
        remainderData.join(" "),
      );
    } else {
      for (const remainder of remainderData) {
        parsedData[remainderKey] = createOrAppend(
          parsedData[remainderKey],
          inferType(remainder, remainderKey),
        );
      }
    }
  }

  return parsedData;
}

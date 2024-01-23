import yargs, { Argv } from "yargs";
import { DatumData } from "../documentControl/DatumDocument";
import { inferType } from "../utils/inferType";
import {
  BaseDataError,
  ExtraDataError,
  MissingRequiredKeyError,
} from "../errors";
import { splitFirst } from "../utils/splitFirst";
import isPlainObject from "lodash.isplainobject";
import { alterDatumData } from "../utils/alterDatumData";
import { datumPath } from "../utils/datumPath";
import get from "lodash.get";
import {
  changeDatumCommand,
  CommandChange,
  commandChanges,
} from "../utils/changeDatumCommand";

export type DataArgs = {
  data?: (string | number)[];
  baseData?: string | DatumData;
  cmdData?: DatumData; // Used for passing special values between commands
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
  const parsedData = baseData
    ? isParsedBaseData(baseData)
      ? baseData
      : inferType(baseData)
    : {};
  if (
    typeof parsedData !== "object" ||
    parsedData === null ||
    Array.isArray(parsedData)
  ) {
    throw new BaseDataError("base data not a valid object");
  }
  return parsedData as DatumData;
}

export function handleDataArgs(args: DataArgs): DatumData {
  args.data ??= [];
  args.required ??= [];
  args.optional ??= [];

  const requiredKeys =
    typeof args.required === "string" ? [args.required] : args.required;
  args.required = requiredKeys;

  const optionalKeys =
    typeof args.optional === "string" ? [args.optional] : args.optional;
  args.optional = optionalKeys;

  const remainderKey =
    args.remainder ??
    (args.commentRemainder
      ? "comment"
      : args.lenient
        ? "extraData"
        : undefined);
  const remainderAsString = args.stringRemainder ?? args.commentRemainder;
  const remainderData = [];

  const datumData = { ...parseBaseData(args.baseData), ...args.cmdData };

  // for idempotence of processing dataArgs
  args.baseData = datumData;
  delete args.cmdData;

  posArgsLoop: while (args.data.length > 0) {
    const arg = args.data.shift()!;
    const [beforeEquals, afterEquals] = splitFirst("=", String(arg));

    if (afterEquals !== undefined) {
      // key is explicitly given e.g., 'key=value'
      const path = datumPath(beforeEquals);
      const value = afterEquals;

      // Search for default value to allow explicitly setting it to default using '.',
      // or use an existing value if there already is one
      const existingValue = get(datumData, path);
      const defaultValue =
        existingValue ??
        optionalKeys
          ?.find((optionalWithDefault) =>
            new RegExp(`^${path}=(.*)$`).test(optionalWithDefault),
          )
          ?.split("=")[1];
      alterDatumData({ datumData, path, value, defaultValue });
      continue posArgsLoop;
    }

    // no explicit key given
    const dataValue = beforeEquals;

    requiredKeysLoop: while (requiredKeys.length > 0) {
      const path = datumPath(requiredKeys.shift()!);

      if (get(datumData, path) !== undefined) {
        continue requiredKeysLoop;
      }

      alterDatumData({ datumData, path: path, value: dataValue });
      continue posArgsLoop;
    }

    optionalKeysLoop: while (optionalKeys.length > 0) {
      const [rawPath, defaultValue] = splitFirst("=", optionalKeys.shift()!);
      const path = datumPath(rawPath);

      // TODO: Consider explicitly setting an optional key to undefined to be sufficient to bypass this step
      if (get(datumData, path) !== undefined) {
        continue optionalKeysLoop;
      }

      if (
        path === "dur" &&
        commandChanges.includes(dataValue as CommandChange)
      ) {
        changeDatumCommand(datumData, dataValue as CommandChange, args);
        continue posArgsLoop;
      }

      alterDatumData({
        datumData,
        path: path,
        value: dataValue,
        defaultValue,
      });
      continue posArgsLoop;
    }

    remainderData.push(dataValue);
  }

  while (requiredKeys.length > 0) {
    const requiredKey = requiredKeys.shift()!;
    const requiredPath = datumPath(requiredKey);
    if (get(datumData, requiredPath) !== undefined) {
      continue;
    }
    // Allow required keys to be given a default value via an optional key
    // This is useful for nested aliases where a key is required in the parent,
    // but then a child creates an optional default value for it
    // e.g. alias tx='datum occur tx -K acc -K amount'
    //      alias rent='tx acc=Checking -k amount=1200'
    // 'rent' would create a tx doc with amount=1200, while 'rent 1500' would create
    // a tx doc with amount=1500
    //TODO: Allow state based keys to still work if defined in separate ways
    if (
      optionalKeys?.find((optionalWithDefault) =>
        new RegExp(`^${requiredKey}=`).test(optionalWithDefault),
      )
    ) {
      continue;
    }
    throw new MissingRequiredKeyError(requiredKey);
  }

  // If optional keys with default values are left assign them
  while (optionalKeys.length > 0) {
    const [rawPath, defaultValue] = splitFirst("=", optionalKeys.shift()!);
    const path = datumPath(rawPath);
    if (
      defaultValue !== undefined &&
      [undefined, "."].includes(get(datumData, path))
    ) {
      alterDatumData({ datumData, path, value: defaultValue });
    }
  }

  if (args.comment) {
    const commentArray = Array.isArray(args.comment)
      ? args.comment
      : [args.comment];
    commentArray.forEach((comment) => {
      alterDatumData({
        datumData,
        path: "comment",
        value: comment,
        append: true,
      });
    });
    delete args.comment;
  }

  if (remainderData.length > 0) {
    if (remainderKey === undefined) {
      // @ts-expect-error overrestrictive includes
      if (commandChanges.includes(remainderData[0])) {
        const command = remainderData.shift() as CommandChange;
        changeDatumCommand(datumData, command, args);
        args.data = remainderData;
        return handleDataArgs(args);
      }
      throw new ExtraDataError(remainderData);
    }

    if (remainderAsString) {
      alterDatumData({
        datumData,
        path: remainderKey,
        value: remainderData.join(" "),
        append: true,
      });
    } else {
      for (const remainder of remainderData) {
        alterDatumData({
          datumData,
          path: remainderKey,
          value: remainder,
          append: true,
        });
      }
    }
  }

  return datumData;
}

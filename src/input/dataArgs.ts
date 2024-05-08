import { DatumData } from "../documentControl/DatumDocument";
import { inferType } from "../utils/inferType";
import { BaseDataError, ExtraDataError } from "../errors";
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
import { jClone } from "../utils/jClone";
import { Action, ArgumentParser, Namespace } from "argparse";
import { consolidateKeys } from "./consolidateKeys";
import { AddCmdArgs } from "../commands/addCmd";

export type DataArgs = {
  data?: (string | number)[];
  baseData?: string | DatumData;
  cmdData?: DatumData; // Used for passing special values between commands
  keys?: string[];
  comment?: string | string[];
  remainder?: string;
  stringRemainder?: boolean;
  commentRemainder?: boolean;
  lenient?: boolean;
};

export const dataArgs = new ArgumentParser({
  add_help: false,
});

const dataGroup = dataArgs.add_argument_group({
  title: "Data",
  description: "Options for specifying data",
});
dataGroup.add_argument("data", {
  help:
    "The data to put in the document. " +
    "Data must include one argument for each key specified by --key/-k. " +
    "If a key is given with an '=' the key is optional and will have a default value of what comes after the '='. " +
    "Optional keys can be skipped over with a '.' " +
    "e.g. `-k req` has 'req' as a required key, `-k opt=` has 'opt' as an optional key, `-k opt=default` has 'opt' as an optional key with a default value of 'default', which will be used if there is no argument or a dot given for it. " +
    "If a key is given multiple times, the last appearing form (required/optional/default) is used in the first appearing position. " +
    "e.g. `-k key -k another=value -k key=default -k third= -k another` is equivalent to `-k key=default -k another -k third=`. " +
    "Use -K to specify a key that should also be used in the id of the document. Equivalent to `-k key --id %key`" +
    'Additional data can be specified in a "key=data" format anywhere in the command. ' +
    "Any data that does not have a key will be put in the key specified with --remainder. If --lenient is specified, defaults to 'extraData'. ",
  nargs: "*",
  type: "str",
});
dataGroup.add_argument("-b", "--base-data", {
  help: "base object on which additional keys are added. Fed through relaxed-json, but must still parse to an object. Use with --no-metadata for raw json input into couchdb. Default: {}",
  type: "str",
  dest: "baseData",
});
dataGroup.add_argument("-c", "--comment", {
  help: "comment to include in the data",
  type: "str",
  action: "append",
});
dataGroup.add_argument("-k", "--key", {
  help: "Add a key to the data, will be filled with first keyless data. Without an '=', the key will be required. With a trailing '=', the key is optional. A default value can be specified with an '=', e.g., -k key=value",
  type: "str",
  action: "append",
  dest: "keys",
});
dataGroup.add_argument("-K", "--id-key", {
  help: "Add a key to the data and also use it as part of the id of the document. Only useful for adding new docs. Equivalent to calling `-k key --id %key`",
  type: "str",
  action: class IdKeyAction extends Action {
    call(
      _parser: ArgumentParser,
      namespace: DataArgs & AddCmdArgs,
      value: string,
      _optionString?: string | null,
    ) {
      const [keyName] = splitFirst("=", value);
      namespace.keys ??= [];
      namespace.idParts ??= [];

      namespace.keys.push(value);
      namespace.idParts.push(`%${keyName}`);
    }
  },
});
dataGroup.add_argument("-R", "--remainder", {
  help: "Any extra data supplied will be put into this key as an array. When --lenient is specified, defaults to 'extraData'",
  type: "str",
});
dataGroup.add_argument("-S", "--string-remainder", {
  help: "Remainder data will be a space-concatenated string rather than an array",
  action: "store_true",
  dest: "stringRemainder",
});
dataGroup.add_argument("-C", "--comment-remainder", {
  help: "All unused data will be joined into a string and stored as a comment. Equivalent to `-SR comment`",
  action: "store_true",
  dest: "commentRemainder",
});
dataGroup.add_argument("-l", "--lenient", {
  help: "Allow extra data without defined keys",
  action: "store_true",
});
dataGroup.add_argument("--strict", {
  help: "Do not allow extra data without defined keys, default behavior. Overrides --lenient",
  action: "store_false",
  dest: "lenient",
});

function isParsedBaseData(baseData: DatumData | string): baseData is DatumData {
  return isPlainObject(baseData);
}

export function parseBaseData(baseData?: DatumData | string): DatumData {
  const parsedData = baseData
    ? isParsedBaseData(baseData)
      ? baseData
      : inferType(baseData)
    : {};
  if (!isPlainObject(parsedData)) {
    throw new BaseDataError("base data not a valid object");
  }
  return parsedData as DatumData;
}

export function handleDataArgs(args: DataArgs): DatumData {
  args = jClone(args); // avoid modifying original args
  args.data ??= [];
  args.keys = consolidateKeys(args.keys ?? []);

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

  // for idempotence of processing dataArgs when called recursively internally
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
      const correspondingKey = args.keys?.find((key) =>
        new RegExp(`^${path}=(.*)$`).test(key),
      );

      const defaultValue =
        existingValue ??
        (correspondingKey ? splitFirst("=", correspondingKey)[1] : undefined);
      alterDatumData({ datumData, path, value, defaultValue });
      continue posArgsLoop;
    }

    // no explicit key given, assign a key from the keys list
    const dataValue = beforeEquals;

    keysLoop: while (args.keys.length > 0) {
      const [rawPath, defaultValue] = splitFirst("=", args.keys.shift()!);
      const isRequired = defaultValue === undefined;
      const path = datumPath(rawPath);

      // TODO: Make the explicit setting of a key to undefined to be sufficient to bypass this step
      if (get(datumData, path) !== undefined) {
        continue keysLoop;
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
        isRequired,
      });
      continue posArgsLoop;
    }

    remainderData.push(dataValue);
  }

  while (args.keys.length > 0) {
    // if optional keys are left, fill them with default values.
    // remaining required keys throw an error
    const [rawPath, defaultValue] = splitFirst("=", args.keys.shift()!);
    const path = datumPath(rawPath);
    const isRequired = defaultValue === undefined;

    const existingValue = get(datumData, path);

    if (existingValue !== undefined && existingValue !== ".") {
      // Manually specified or already exists in data
      continue;
    }
    alterDatumData({ datumData, path, value: defaultValue, isRequired });
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

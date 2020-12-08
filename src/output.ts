import { CouchDocument } from "./types";
const chalk = require("chalk");
const { destructureIdKeys } = require("./ids");

const stringify = require("string.ify");

const ACTION_CHALK: { [key: string]: (val: any) => string } = {
  CREATE: chalk.green,
  DELETE: chalk.red,
  EXISTS: chalk.yellow,
  default: chalk.white,
};

const displayDoc = (doc: CouchDocument, action?: string) => {
  const color = action
    ? ACTION_CHALK[action] ?? ACTION_CHALK.default
    : ACTION_CHALK.default;

  const actionText = action ? chalk.grey(action + ": ") : "";
  console.log(actionText + color(doc._id));

  const maxLength = process.stdout.columns;
  const docClone = JSON.parse(JSON.stringify(doc));
  delete docClone._id;
  delete docClone._rev;
  delete docClone.meta;
  console.log(
    stringify.configure({
      formatter: (x: any) =>
        typeof x === "string"
          ? color(x)
          : typeof x === "number"
          ? chalk.bold(color(x))
          : undefined,
      maxLength: maxLength,
    })(docClone)
  );
};

module.exports = { displayDoc };

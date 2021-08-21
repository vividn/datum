import {
  DatumData,
  EitherDocument,
  EitherPayload,
  isDatumDocument,
  isDatumPayload,
} from "./documentControl/DatumDocument";
import chalk from "chalk";
import stringify from "string.ify";

enum ACTIONS {
  Create = "CREATE",
  Delete = "DELETE",
  Exists = "EXISTS",
  Update = "UPDATE",
  OWrite = "OWRITE",
  Rename = "RENAME",
  NoDiff = "NODIFF",
  Failed = "FAILED",
}
const ACTION_CHALK: { [key in ACTIONS]: (val: any) => string } = {
  CREATE: chalk.green,
  DELETE: chalk.red,
  EXISTS: chalk.yellow,
  UPDATE: chalk.cyan,
  OWRITE: chalk.blue,
  RENAME: chalk.cyan,
  NODIFF: chalk.hex("#ffa500"),
  FAILED: chalk.red,
};

const actionId = (action: ACTIONS, id: string): string => {
  const color = ACTION_CHALK[action];
  const actionText = chalk.grey(action + ": ");
  return actionText + color(id);
};

export const displayData = (
  data: DatumData,
  color: (val: any) => string
): void => {
  const maxLength = process.stdout.columns;
  console.log(
    stringify.configure({
      formatter: (x: any) =>
        typeof x === "string"
          ? color(x)
          : typeof x === "number"
          ? chalk.bold(color(x))
          : undefined,
      maxLength: maxLength,
    })(data)
  );
};

export const showCreate = (doc: EitherDocument, showAll = false): void => {
  console.log(actionId(ACTIONS.Create, doc._id));
  if (isDatumDocument(doc) && !showAll) {
    displayData(doc.data, ACTION_CHALK["CREATE"]);
  } else {
    displayData(doc, ACTION_CHALK["CREATE"]);
  }
};

export const showExists = (doc: EitherDocument, showAll = false): void => {
  console.log(actionId(ACTIONS.Exists, doc._id));
  if (isDatumDocument(doc) && !showAll) {
    displayData(doc.data, ACTION_CHALK["EXISTS"]);
  } else {
    displayData(doc, ACTION_CHALK["EXISTS"]);
  }
};

export const showUpdate = (
  beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  showAll = false
): void => {
  console.log(actionId(ACTIONS.Update, afterDoc._id));
  if (isDatumDocument(afterDoc) && !showAll) {
    displayData(afterDoc.data, ACTION_CHALK["UPDATE"]);
  } else {
    displayData(afterDoc, ACTION_CHALK["UPDATE"]);
  }
};

export const showOWrite = (
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  showAll = false
): void => {
  console.log(actionId(ACTIONS.OWrite, afterDoc._id));
  if (isDatumDocument(afterDoc) && !showAll) {
    displayData(afterDoc.data, ACTION_CHALK["OWRITE"]);
  } else {
    displayData(afterDoc, ACTION_CHALK["OWRITE"]);
  }
};

export const showRename = (
  beforeId: string,
  afterId: string,
  _showAll = false
): void => {
  console.log(
    actionId(ACTIONS.Rename, beforeId) + " ⟶ " + chalk.green(afterId)
  );
};

export const showNoDiff = (doc: EitherDocument, showAll = false): void => {
  console.log(actionId(ACTIONS.NoDiff, doc._id));
  if (isDatumDocument(doc) && !showAll) {
    displayData(doc.data, ACTION_CHALK["NODIFF"]);
  } else {
    displayData(doc, ACTION_CHALK["NODIFF"]);
  }
};

export const showFailed = (doc: EitherPayload, showAll = false): void => {
  console.log(actionId(ACTIONS.Failed, ""));
  if (isDatumPayload(doc) && !showAll) {
    displayData(doc.data, ACTION_CHALK["FAILED"]);
  } else {
    displayData(doc, ACTION_CHALK["FAILED"]);
  }
};

export enum Show {
  None,
  Minimal,
  Standard,
  All,
}

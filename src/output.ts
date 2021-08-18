import {
  DatumData,
  EitherDocument,
  isDatumDocument,
} from "./documentControl/DatumDocument";
import chalk from "chalk";
import stringify from "string.ify";

enum ACTIONS {
  Create = "CREATE",
  Delete = "DELETE",
  Exists = "EXISTS",
  Update = "UPDATE",
  NoDiff = "NODIFF"
}
const ACTION_CHALK: { [key in ACTIONS]: (val: any) => string } = {
  CREATE: chalk.green,
  DELETE: chalk.red,
  EXISTS: chalk.yellow,
  UPDATE: chalk.cyan,
  NODIFF: chalk.hex("#ffa500")
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
  if (beforeDoc._id !== afterDoc._id) {
    console.log(
      actionId(ACTIONS.Update, beforeDoc._id) +
        " ⟶ " +
        chalk.green(afterDoc._id)
    );
  } else {
    console.log(actionId(ACTIONS.Update, afterDoc._id));
  }
  if (isDatumDocument(afterDoc) && !showAll) {
    displayData(afterDoc.data, ACTION_CHALK["UPDATE"]);
  } else {
    displayData(afterDoc, ACTION_CHALK["UPDATE"]);
  }
};

export const showNoDiff = (
  doc: EitherDocument,
  showAll = false
): void => {
  console.log(actionId(ACTIONS.NoDiff, doc._id));
  if (isDatumDocument(doc) && !showAll) {
    displayData(doc.data, ACTION_CHALK["NODIFF"]);
  } else {
    displayData(doc, ACTION_CHALK["NODIFF"]);
  }
}

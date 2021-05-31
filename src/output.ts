import {
  DataOnlyDocument,
  DatumData,
  DatumDocument,
  isDatumDocument,
} from "./documentControl/DatumDocument";
import chalk from "chalk";
import stringify from "string.ify";

enum ACTIONS {
  Create = "CREATE",
  Delete = "DELETE",
  Exists = "EXISTS",
}
const ACTION_CHALK: { [key in ACTIONS]: (val: any) => string } = {
  CREATE: chalk.green,
  DELETE: chalk.red,
  EXISTS: chalk.yellow,
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

export const showCreate = (
  doc: DatumDocument | DataOnlyDocument,
  showAll = false
): void => {
  console.log(actionId(ACTIONS.Create, doc._id));
  if (isDatumDocument(doc) && !showAll) {
    displayData(doc.data, ACTION_CHALK["CREATE"]);
  } else {
    displayData(doc, ACTION_CHALK["CREATE"]);
  }
};

export const showExists = (
  doc: DatumDocument | DataOnlyDocument,
  showAll = false
): void => {
  console.log(actionId(ACTIONS.Exists, doc._id));
  if (isDatumDocument(doc) && !showAll) {
    displayData(doc.data, ACTION_CHALK["EXISTS"]);
  } else {
    displayData(doc, ACTION_CHALK["EXISTS"]);
  }
};

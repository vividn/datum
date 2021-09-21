import {
  DatumData,
  EitherDocument,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import chalk from "chalk";
import stringify from "string.ify";
import jClone from "../utils/jClone";

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

function actionId(action: ACTIONS, id: string): string {
  const color = ACTION_CHALK[action];
  const actionText = chalk.grey(action + ": ");
  return actionText + color(id);
}

export function displayData(
  data: DatumData,
  color: (val: any) => string
): void {
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
}

export function showRename(
  beforeId: string,
  afterId: string,
  show: Show
): void {
  if (show === Show.None) {
    return;
  }
  console.log(
    actionId(ACTIONS.Rename, beforeId) + " ⟶ " + chalk.green(afterId)
  );
}

export function showSingle(
  action: ACTIONS,
  doc: EitherPayload,
  show: Show
): void {
  if (show === Show.None) {
    return;
  }

  console.log(actionId(action, doc._id ?? ""));
  if (show === Show.Minimal) {
    return;
  }

  if (show === Show.All) {
    displayData(doc, ACTION_CHALK[action]);
    return;
  }

  if (show === Show.Standard) {
    const docClone = jClone(doc);
    delete docClone._id;
    delete docClone._rev;
    if (isDatumPayload(docClone)) {
      displayData(docClone.data, ACTION_CHALK[action]);
    } else {
      displayData(docClone, ACTION_CHALK[action]);
    }
  }
}
export function showCreate(doc: EitherDocument, show: Show): void {
  return showSingle(ACTIONS.Create, doc, show);
}
export function showExists(doc: EitherDocument, show: Show): void {
  return showSingle(ACTIONS.Exists, doc, show);
}
export function showNoDiff(doc: EitherDocument, show: Show): void {
  return showSingle(ACTIONS.NoDiff, doc, show);
}
export function showFailed(payload: EitherPayload, show: Show): void {
  return showSingle(ACTIONS.Failed, payload, show);
}
export function showDelete(payload: EitherPayload, show: Show): void {
  return showSingle(ACTIONS.Delete, payload, show);
}

export function showUpdate(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  show: Show
): void {
  return showSingle(ACTIONS.Update, afterDoc, show);
}
export function showOWrite(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  show: Show
): void {
  return showSingle(ACTIONS.OWrite, afterDoc, show);
}

export enum Show {
  None = "none",
  Minimal = "minimal",
  Standard = "standard",
  All = "all",
}

import {
  DatumData,
  EitherDocument,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import chalk from "chalk";
import stringify from "string.ify";
import { jClone } from "../utils/jClone";
import { assembleId } from "../ids/assembleId";
import { OutputArgs, Show } from "../input/outputArgs";

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

function actionId(action: ACTIONS, id: string, humanId?: string): string {
  const color = ACTION_CHALK[action];
  const actionText = chalk.grey(action + ": ");
  const quickId = humanId ? ` (${humanId.slice(0, 5)})` : "";
  return actionText + color(id) + quickId;
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

export function showCustomFormat(
  data: EitherPayload,
  formatString: string,
  color: (val: any) => string
): void {
  console.log(color(assembleId({ payload: data, idStructure: formatString })));
}

export function showRename(
  beforeId: string,
  afterId: string,
  args: OutputArgs
): void {
  const { show } = args;
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
  args: OutputArgs
): void {
  const { show, formatString } = args;
  const color = ACTION_CHALK[action];

  if (show === Show.None) {
    return;
  }

  if (show === Show.Format) {
    if (formatString === undefined) {
      throw new Error(
        "MissingArgument: formatted show requested without a format string"
      );
    }
    showCustomFormat(doc, formatString, color);
  }

  console.log(actionId(action, doc._id ?? "", doc.meta?.humanId));
  if (show === Show.Minimal) {
    return;
  }

  if (formatString) {
    showCustomFormat(doc, formatString, color);
    if (show === Show.Default) {
      return;
    }
  }

  if (show === Show.All) {
    displayData(doc, color);
    return;
  }

  if (
    show === Show.Standard ||
    (show === Show.Default && formatString === undefined)
  ) {
    const docClone = jClone(doc);
    delete docClone._id;
    delete docClone._rev;
    if (isDatumPayload(docClone)) {
      displayData(docClone.data, color);
    } else {
      displayData(docClone, color);
    }
  }
}
export function showCreate(doc: EitherDocument, args: OutputArgs): void {
  return showSingle(ACTIONS.Create, doc, args);
}
export function showExists(doc: EitherDocument, args: OutputArgs): void {
  return showSingle(ACTIONS.Exists, doc, args);
}
export function showNoDiff(doc: EitherDocument, args: OutputArgs): void {
  return showSingle(ACTIONS.NoDiff, doc, args);
}
export function showFailed(payload: EitherPayload, args: OutputArgs): void {
  return showSingle(ACTIONS.Failed, payload, args);
}
export function showDelete(payload: EitherPayload, args: OutputArgs): void {
  return showSingle(ACTIONS.Delete, payload, args);
}

export function showUpdate(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  args: OutputArgs
): void {
  return showSingle(ACTIONS.Update, afterDoc, args);
}
export function showOWrite(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  args: OutputArgs
): void {
  return showSingle(ACTIONS.OWrite, afterDoc, args);
}

import {
  DatumData,
  DatumMetadata,
  EitherDocument,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import chalk from "chalk";
import stringify from "string.ify";
import { jClone } from "../utils/jClone";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";

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
  payload: EitherPayload,
  formatString: string,
  color: (val: any) => string
): void {
  let data: DatumData;
  let meta: DatumMetadata | undefined;
  if (isDatumPayload(payload)) {
    data = payload.data as DatumData;
    meta = payload.meta;
  } else {
    data = payload as DatumData;
  }
  const outputString = interpolateFields({ data, meta, format: formatString });
  console.log(color(outputString));
}

export function showRename(
  beforeId: string,
  afterId: string,
  outputArgs: OutputArgs
): void {
  const { show } = sanitizeOutputArgs(outputArgs);
  if (show === Show.None || show === Show.Format) {
    return;
  }
  console.log(
    actionId(ACTIONS.Rename, beforeId) + " ⟶ " + chalk.green(afterId)
  );
}

export function showSingle(
  action: ACTIONS,
  doc: EitherPayload,
  outputArgs: OutputArgs
): void {
  const { show, formatString } = sanitizeOutputArgs(outputArgs);
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
export function showCreate(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.Create, doc, outputArgs);
}
export function showExists(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.Exists, doc, outputArgs);
}
export function showNoDiff(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.NoDiff, doc, outputArgs);
}
export function showFailed(
  payload: EitherPayload,
  outputArgs: OutputArgs
): void {
  return showSingle(ACTIONS.Failed, payload, outputArgs);
}
export function showDelete(
  payload: EitherPayload,
  outputArgs: OutputArgs
): void {
  return showSingle(ACTIONS.Delete, payload, outputArgs);
}

export function showUpdate(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  outputArgs: OutputArgs
): void {
  return showSingle(ACTIONS.Update, afterDoc, outputArgs);
}
export function showOWrite(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  outputArgs: OutputArgs
): void {
  return showSingle(ACTIONS.OWrite, afterDoc, outputArgs);
}

function sanitizeOutputArgs(outputArgs: OutputArgs): {
  show: Show;
  formatString?: string;
} {
  const show =
    (outputArgs.showAll ? Show.All : outputArgs.show) ??
    (outputArgs.formatString ? Show.Format : Show.None);
  return { show, formatString: outputArgs.formatString };
}

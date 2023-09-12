import {
  DatumData,
  DatumMetadata,
  EitherDocument,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import chalk, { Chalk } from "chalk";
import stringify from "string.ify";
import { jClone } from "../utils/jClone";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import { DateTime, Duration } from "luxon";
import { getTimezone } from "../time/getTimezone";

chalk.level = 3;

enum ACTIONS {
  Create = "CREATE",
  Delete = "DELETE",
  Exists = "EXISTS",
  Update = "UPDATE",
  OWrite = "OWRITE",
  Rename = "RENAME",
  NoDiff = "NODIFF",
  Failed = "FAILED",
  Start = "START↦",
  End = "⇥ END",
  Switch = "SWITCH",
}
const ACTION_CHALK: { [key in ACTIONS]: Chalk } = {
  [ACTIONS.Create]: chalk.green,
  [ACTIONS.Delete]: chalk.red,
  [ACTIONS.Exists]: chalk.yellow,
  [ACTIONS.Update]: chalk.cyan,
  [ACTIONS.OWrite]: chalk.blue,
  [ACTIONS.Rename]: chalk.cyan,
  [ACTIONS.NoDiff]: chalk.hex("#ffa500"),
  [ACTIONS.Failed]: chalk.red,
  [ACTIONS.Start]: chalk.hex("#a5ffa5"),
  [ACTIONS.End]: chalk.hex("#a5ffa5"),
  [ACTIONS.Switch]: chalk.hex("#a5ffa5"),
};

function actionId(action: ACTIONS, id: string, humanId?: string): string {
  const color = ACTION_CHALK[action];
  const actionText = color.inverse(` ${action} `);
  const quickId = humanId ? ` (${humanId.slice(0, 5)})` : "";
  return actionText + color(id) + quickId;
}

function headerLine(action: ACTIONS, doc: EitherDocument): string {
  const { data } = pullOutData(doc);
  const id = doc._id;
  const color = ACTION_CHALK[action];
  const actionText = color.inverse(` ${action} `);
  const fieldOrPartitionText = color(
    data.field ? data.field : id.split(":")[0]
  );
  const stateText = ![undefined, true, false].includes(data.state)
    ? ` ${data.state}`
    : "";
  const duration = Duration.fromISO(data.dur ?? data.duration);
  const durationText = duration.isValid ? duration.toFormat(" ⟝m'm'⟞") : "";
  const idText = id === fieldOrPartitionText ? "" : ` ${chalk.dim(id)}`;
  return actionText + fieldOrPartitionText + stateText + durationText + idText;
}

function footerLine(action: ACTIONS, doc: EitherDocument): string {
  const { data, meta } = pullOutData(doc);
  const hidText = meta?.humanId ? `(${meta.humanId.slice(0, 5)}) ` : "";
  const occurTime = DateTime.fromISO(data.occurTime ?? "");
  const occurTimeText = occurTime.isValid
    ? occurTime
        .setZone(getTimezone(data.occurUtcOffset))
        .toFormat("yyyy-MM-dd HH:mm:ss Z")
    : "";

  return hidText + occurTimeText;
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
  doc: EitherDocument,
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

  console.log(headerLine(action, doc));
  if (show === Show.Minimal) {
    console.log(footerLine(action, doc));
    return;
  }

  if (formatString) {
    showCustomFormat(doc, formatString, color);
  }

  if (show === Show.All) {
    displayData(doc, color);
  }

  if (
    show === Show.Standard ||
    (show === Show.Default && formatString === undefined)
  ) {
    const docClone = jClone(doc);
    if (isDatumPayload(docClone)) {
      displayData(docClone.data, color);
    } else {
      displayData(docClone, color);
    }
  }

  console.log(footerLine(action, doc));
}
export function showCreate(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.Create, doc, outputArgs);
}

export function showStart(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.Start, doc, outputArgs);
}

export function showEnd(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.End, doc, outputArgs);
}

export function showSwitch(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.Switch, doc, outputArgs);
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
  return showSingle(
    ACTIONS.Failed,
    { _id: "", _rev: "", ...payload } as EitherDocument,
    outputArgs
  );
}
export function showDelete(
  payload: EitherPayload,
  outputArgs: OutputArgs
): void {
  return showSingle(
    ACTIONS.Delete,
    { _id: "", _rev: "", ...payload } as EitherDocument,
    outputArgs
  );
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
    outputArgs.show === Show.None
      ? Show.None
      : (outputArgs.showAll ? Show.All : outputArgs.show) ??
        (outputArgs.formatString ? Show.Format : Show.None);
  return { show, formatString: outputArgs.formatString };
}

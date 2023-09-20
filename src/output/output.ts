import {
  DatumData,
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import chalk, { Chalk } from "chalk";
import stringify from "string.ify";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import { DateTime, Duration } from "luxon";
import { getTimezone } from "../time/getTimezone";
import { DatumState } from "../views/datumViews/activeStateView";

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
  End = "⟞  END",
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

function formatOccurTime(
  occurTime?: string,
  occurUtcOffset?: string | number
): string | undefined {
  if (!occurTime) {
    return undefined;
  }
  // if occurTime is just a date, then return it
  if (!occurTime.includes("T")) {
    return occurTime;
  }

  const dateTime = DateTime.fromISO(occurTime, {
    zone: getTimezone(occurUtcOffset),
  });
  if (!dateTime.isValid) {
    return undefined;
  }

  const dateText = dateTime.toISODate();
  const offsetText = chalk.dim(dateTime.toFormat("('U'Z)"));
  const timeText = dateTime.toFormat("HH:mm:ss");
  return [dateText, offsetText, timeText].filter(Boolean).join(" ");
}

function formatStateInfo(
  state?: DatumState,
  lastState?: DatumState
): string | undefined {
  const lastStateText =
    lastState !== undefined
      ? lastState === state
        ? chalk.red(`${lastState}→`)
        : chalk.dim(`${lastState}→`)
      : "";
  return state !== undefined ? ` ${lastStateText}${state}` : undefined;
}

function formatDuration(dur?: string | undefined): string | undefined {
  const duration = Duration.fromISO(dur || "");
  return duration.isValid ? duration.toFormat(" ⟝ m'm'⟞ ") : undefined;
}
function formattedNonRedundantData(data: DatumData): string | undefined {
  const {
    state: _state,
    lastState: _lastState,
    occurTime: _occurTime,
    occurUtcOffset: _occurUtcOffset,
    dur: _dur,
    duration: _duration,
    field: _field,
    ...filteredData
  } = data;
  if (Object.keys(filteredData).length === 0) {
    return undefined;
  }
  const formatted = stringify(filteredData);
  // replace starting and ending curly braces with spaces
  formatted[0] = " ";
  formatted[formatted.length - 1] = " ";
  return formatted;
}

type ExtractedAndFormatted = {
  actionText: string;
  idText?: string;
  hidText?: string;
  occurTimeText?: string;
  fieldText?: string;
  stateText?: string;
  durText?: string;
  nonRedundantData?: string;
  entireDocument: string;
};
function extractFormatted(
  action: ACTIONS,
  doc: EitherPayload
): ExtractedAndFormatted {
  const color = ACTION_CHALK[action];
  const { data, meta } = pullOutData(doc);

  return {
    actionText: color.inverse(` ${action} `),
    idText: doc._id ?? chalk.dim(doc._id),
    hidText: meta?.humanId ? `(${meta.humanId.slice(0, 5)})` : undefined,
    occurTimeText: formatOccurTime(data.occurTime, data.occurUtcOffset),
    fieldText: data?.field ?? color(data.field),
    stateText: formatStateInfo(data.state, data.lastState),
    durText: formatDuration(data.dur ?? data.duration),
    nonRedundantData: formattedNonRedundantData(data),
    entireDocument: stringify(doc),
  };
}

function actionId(action: ACTIONS, id: string, humanId?: string): string {
  const color = ACTION_CHALK[action];
  const actionText = color.inverse(` ${action} `);
  const quickId = humanId ? ` (${humanId.slice(0, 5)})` : "";
  return actionText + color(id) + quickId;
}

function showHeaderLine(formatted: ExtractedAndFormatted): void {
  console.log(
    [formatted.actionText, formatted.idText, formatted.hidText]
      .filter(Boolean)
      .join(" ")
  );
}

function showFooterLine(formatted: ExtractedAndFormatted): void {
  const footerLine = [
    formatted.occurTimeText,
    formatted.fieldText,
    formatted.stateText,
    formatted.durText,
  ]
    .filter(Boolean)
    .join(" ");
  if (footerLine !== "") {
    console.log(footerLine);
  }
}

export function showCustomFormat(
  payload: EitherPayload,
  formatString: string
): void {
  const { data, meta } = pullOutData(payload);
  const outputString = interpolateFields({ data, meta, format: formatString });
  console.log(outputString);
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
  const extracted = extractFormatted(action, doc);

  if (show === Show.None) {
    return;
  }

  if (show === Show.Format) {
    if (formatString === undefined) {
      throw new Error(
        "MissingArgument: formatted show requested without a format string"
      );
    }
    showCustomFormat(doc, formatString);
  }

  showHeaderLine(extracted);
  if (show === Show.Minimal) {
    return;
  }

  if (formatString) {
    showCustomFormat(doc, formatString);
  }

  if (show === Show.All) {
    console.log(extracted.entireDocument);
  }

  if (
    show === Show.Standard ||
    (show === Show.Default && formatString === undefined)
  ) {
    if (extracted.nonRedundantData !== undefined) {
      console.log(extracted.nonRedundantData);
    }
  }

  showFooterLine(extracted);
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
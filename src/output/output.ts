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

  const date = dateTime.toISODate();
  const dateText =
    date === DateTime.now().toISODate() ? "" : dateTime.toISODate();
  const offsetText = chalk.dim(dateTime.toFormat("Z"));
  const timeText = dateTime.toFormat("HH:mm:ss") + offsetText;
  const fullText = [dateText, timeText].filter(Boolean).join(" ");
  return dateTime > DateTime.now() ? chalk.underline(fullText) : fullText;
}

function formatStateInfo(
  state?: DatumState,
  lastState?: DatumState
): string | undefined {
  if (state === true && lastState === false) {
    return chalk.bold("start");
  }
  if (state === false && lastState === true) {
    return chalk.bold("end");
  }
  const lastStateText =
    lastState !== undefined
      ? lastState === state
        ? chalk.red(`${lastState}⇾`)
        : chalk.dim(`${lastState}⇾`)
      : "";
  return state !== undefined
    ? ` ${lastStateText} ${chalk.bold(state)}`
    : undefined;
}

function formatDuration(
  dur?: string | undefined,
  invert = false
): string | undefined {
  const duration = Duration.fromISO(dur || "");
  if (!duration.isValid) {
    return undefined;
  }
  return duration < Duration.fromMillis(0) !== invert
    ? duration.toFormat(" ⟞ m'm' ⟝ ")
    : duration.toFormat(" ⟝ m'm'⟞ ");
}
function formattedNonRedundantData(data: DatumData): string | undefined {
  const {
    _id,
    _rev,
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
  const stringified = stringify(filteredData);
  // replace starting and ending curly braces with spaces
  const formatted = stringified.replace(/^\{\n?/, " ").replace(/\n?\}$/, " ");
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
    actionText: color(`${action}`),
    idText: doc._id ? chalk.dim(doc._id) : undefined,
    hidText: meta?.humanId ? color(meta.humanId.slice(0, 5)) : undefined,
    occurTimeText: formatOccurTime(data.occurTime, data.occurUtcOffset),
    fieldText: data?.field ? color.inverse(data.field) : undefined,
    stateText: formatStateInfo(data.state, data.lastState),
    durText: formatDuration(data.dur ?? data.duration, data.state === false),
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
    [formatted.actionText, formatted.hidText, formatted.idText]
      .filter(Boolean)
      .join(" ")
  );
}

function showMainInfoLine(formatted: ExtractedAndFormatted): void {
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
    return;
  }

  showHeaderLine(extracted);
  if (show === Show.Minimal) {
    return;
  }
  showMainInfoLine(extracted);

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

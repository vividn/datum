import {
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import chalk, { Chalk } from "chalk";
import stringify from "string.ify";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import { DateTime, Duration, Interval } from "luxon";
import { DatumTime, datumTimeToLuxon } from "../time/timeUtils";
import {
  DatumState,
  isStateObject,
  StateObject,
} from "../state/normalizeState";
import isEqual from "lodash.isequal";

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

export function humanFormattedTime(
  time?: DatumTime | string,
): string | undefined {
  if (!time) {
    return undefined;
  }
  // TODO: remove this once all docs are updated to use DatumTime
  if (typeof time === "string") {
    time = { utc: time };
  }
  // if time is just a date, then return it
  if (!time.utc.includes("T")) {
    const future = time.utc > (DateTime.now().toISODate() ?? time.utc);
    return future ? chalk.underline(time.utc) : time.utc;
  }

  const dateTime = datumTimeToLuxon(time);
  if (dateTime === undefined || !dateTime.isValid) {
    return undefined;
  }

  let dateText: string;
  const date = dateTime.toISODate() as string;
  const today = DateTime.now().toISODate() as string;
  if (date === today) {
    dateText = "";
  } else if (date < today) {
    const days = Interval.fromDateTimes(
      DateTime.fromISO(date),
      DateTime.fromISO(today),
    ).toDuration("days").days;
    dateText = days > 3 ? date : "-" + days + "d";
  } else {
    const days = Interval.fromDateTimes(
      DateTime.fromISO(today),
      DateTime.fromISO(date),
    ).toDuration("days").days;
    dateText = days > 3 ? date : "+" + days + "d";
  }
  const offsetText = chalk.gray(dateTime.toFormat("Z"));
  const timeText = dateTime.toFormat("HH:mm:ss") + offsetText;
  const fullText = [dateText, timeText].filter(Boolean).join(" ");
  const future = dateTime > DateTime.now();
  return future ? chalk.underline(fullText) : fullText;
}

type AllTimes = {
  hybrid?: string;
  occur?: string;
  modify?: string;
  create?: string;
  none: undefined;
};
function formatAllTimes(doc: EitherPayload): AllTimes {
  const { data, meta } = pullOutData(doc);
  const hybrid = data.occurTime
    ? humanFormattedTime(data.occurTime)
    : meta?.createTime
      ? chalk.gray("c") + humanFormattedTime(meta.createTime)
      : undefined;
  const times = {
    hybrid: hybrid,
    occur: humanFormattedTime(data.occurTime),
    modify: chalk.gray("m") + humanFormattedTime(meta?.modifyTime),
    create: chalk.grey("c") + humanFormattedTime(meta?.createTime),
    none: undefined,
  };
  return times;
}

function formatState(state?: DatumState): string | undefined {
  if (state === undefined) {
    return undefined;
  }
  if (state === null) {
    return chalk.bold("null");
  }
  if (Array.isArray(state)) {
    return state.map(formatState).join(",");
  }
  if (isStateObject(state)) {
    const stateId = (state as StateObject).id;
    if (stateId !== undefined) {
      return `{${stateId}}`;
    }
    return "{}";
  }
  if (state === true) {
    return chalk.bold("start");
  }
  if (state === false) {
    return chalk.bold("end");
  }
  return state;
}
function formatStateTransition(
  state?: DatumState,
  lastState?: DatumState,
): string | undefined {
  if (
    (state === true && lastState === false) ||
    (state === false && lastState === true)
  ) {
    return formatState(state);
  }
  const lastStateText =
    lastState !== undefined
      ? isEqual(lastState, state)
        ? chalk.yellow(`${formatState(lastState)}⇾`)
        : chalk.gray(`${formatState(lastState)}⇾`)
      : "";
  return state !== undefined
    ? `${lastStateText} ${formatState(state)}`
    : undefined;
}

function formatDuration(
  dur?: string | undefined,
  invert = false,
): string | undefined {
  const duration = Duration.fromISO(dur || "");
  if (!duration.isValid) {
    return undefined;
  }
  return duration < Duration.fromMillis(0) !== invert
    ? duration.toFormat(" ⟞ m'm' ⟝ ")
    : duration.toFormat(" ⟝ m'm'⟞ ");
}
function formattedNonRedundantData(doc: EitherPayload): string | undefined {
  const { data } = pullOutData(doc);
  const {
    _id,
    _rev,
    state,
    lastState: _lastState,
    occurTime: _occurTime,
    dur: _dur,
    duration: _duration,
    field: _field,
    ...filteredData
  } = data;
  const isStateComplex = !!(
    state &&
    ((Array.isArray(state) && state.some(isStateObject)) ||
      (!Array.isArray(state) && isStateObject(state)))
  );
  const dataToDisplay = isStateComplex
    ? { state, ...filteredData }
    : filteredData;
  const isExtraData = Object.keys(dataToDisplay).length > 0;
  if (!isExtraData) {
    return undefined;
  }
  // replace starting and ending curly braces with spaces
  const formatted = stringify(dataToDisplay)
    .replace(/^\{\n?/, " ")
    .replace(/\n?\}$/, " ");
  return formatted;
}

function formattedDoc(doc: EitherPayload): string {
  return stringify(doc);
}

type ExtractedAndFormatted = {
  action: string;
  id?: string;
  hid?: string;
  time: AllTimes;
  field?: string;
  state?: string;
  stateTransition?: string;
  dur?: string;
};
export function extractFormatted(
  doc: EitherPayload,
  action?: ACTIONS,
): ExtractedAndFormatted {
  const color = action ? ACTION_CHALK[action] : chalk;
  const { data, meta } = pullOutData(doc);
  return {
    action: color(`${action}`),
    id: doc._id ? chalk.gray(doc._id) : undefined,
    hid: meta?.humanId ? color(meta.humanId.slice(0, 5)) : undefined,
    time: formatAllTimes(doc),
    field: data?.field ? color.inverse(data.field) : undefined,
    state: formatState(data.state),
    stateTransition: formatStateTransition(data.state, data.lastState),
    dur: formatDuration(data.dur ?? data.duration, data.state === false),
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
    [formatted.action, formatted.hid, formatted.id].filter(Boolean).join(" "),
  );
}

function showMainInfoLine(formatted: ExtractedAndFormatted): void {
  const footerLine = [
    formatted.time.occur,
    formatted.field,
    formatted.stateTransition,
    formatted.dur,
  ]
    .filter(Boolean)
    .join(" ");
  if (footerLine !== "") {
    console.log(footerLine);
  }
}

export function showCustomFormat(
  payload: EitherPayload,
  formatString: string,
): void {
  const { data, meta } = pullOutData(payload);
  const outputString = interpolateFields({ data, meta, format: formatString });
  console.log(outputString);
}

export function showRename(
  beforeId: string,
  afterId: string,
  outputArgs: OutputArgs,
): void {
  const { show } = sanitizeOutputArgs(outputArgs);
  if (show === Show.None || show === Show.Format) {
    return;
  }
  console.log(
    actionId(ACTIONS.Rename, beforeId) + " ⟶ " + chalk.green(afterId),
  );
}

export function showSingle(
  action: ACTIONS,
  doc: EitherDocument,
  outputArgs: OutputArgs,
): void {
  const { show, formatString } = sanitizeOutputArgs(outputArgs);
  const extracted = extractFormatted(doc, action);

  if (show === Show.None) {
    return;
  }

  if (show === Show.Format) {
    if (formatString === undefined) {
      throw new Error(
        "MissingArgument: formatted show requested without a format string",
      );
    }
    showCustomFormat(doc, formatString);
    return;
  }

  if (show === Show.Minimal) {
    if (action !== ACTIONS.NoDiff) {
      showHeaderLine(extracted);
    }
    return;
  }
  showHeaderLine(extracted);
  showMainInfoLine(extracted);

  if (formatString) {
    showCustomFormat(doc, formatString);
  }

  if (show === Show.All) {
    console.log(formattedDoc(doc));
  }

  if (
    show === Show.Standard ||
    (show === Show.Default && formatString === undefined)
  ) {
    const formattedData = formattedNonRedundantData(doc);
    if (formattedData !== undefined) {
      console.log(formattedData);
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
  outputArgs: OutputArgs,
): void {
  return showSingle(
    ACTIONS.Failed,
    { _id: "", _rev: "", ...payload } as EitherDocument,
    outputArgs,
  );
}
export function showDelete(
  payload: EitherPayload,
  outputArgs: OutputArgs,
): void {
  return showSingle(
    ACTIONS.Delete,
    { _id: "", _rev: "", ...payload } as EitherDocument,
    outputArgs,
  );
}

export function showUpdate(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  outputArgs: OutputArgs,
): void {
  return showSingle(ACTIONS.Update, afterDoc, outputArgs);
}
export function showOWrite(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  outputArgs: OutputArgs,
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

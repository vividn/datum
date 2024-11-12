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
import { Duration } from "luxon";
import { isStateObject, StateObject } from "../state/normalizeState";
import isEqual from "lodash.isequal";
import { humanTime } from "../time/humanTime";
import {
  fieldChalk,
  getFieldColor,
  getStateColor,
  stateChalk,
} from "../field/fieldColor";
import { getContrastTextColor } from "../utils/colorUtils";

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
    ? humanTime(data.occurTime)
    : meta?.createTime
      ? chalk.gray("c") + humanTime(meta.createTime)
      : undefined;
  const times = {
    hybrid: hybrid,
    occur: humanTime(data.occurTime),
    modify: chalk.gray("m") + humanTime(meta?.modifyTime),
    create: chalk.grey("c") + humanTime(meta?.createTime),
    none: undefined,
  };
  return times;
}

function formatField(field?: string): string | undefined {
  if (field === undefined) {
    return undefined;
  }
  return fieldChalk({ field })(field);
}

const DOT = "●" as const;
const NON_OCCUR = "¢" as const;
const SHADING = "▞" as const;
const WARNING = "!" as const;
const NULL = "∅" as const;

function formatState(data: DatumData): string | undefined {
  const { state, lastState, field, occurTime, dur } = data;

  if (field === undefined && occurTime === undefined) {
    return;
  }

  const fieldColor = getFieldColor(field);
  const stateColor = getStateColor({ field, state });

  const isNonOccur = occurTime === undefined;
  const isNegativeDur = Duration.fromISO(dur || "").as("milliseconds") < 0;
  const isPoint = dur === null || (dur === undefined && state === undefined);
  const isFalseState = state === false;
  const noStateChange = isEqual(state, lastState) && state !== undefined;

  const beforeChalk = stateChalk({ field, state: lastState });
  let beforeText: string;
  if (occurTime === undefined) {
    beforeText = chalk.hex(fieldColor)(NON_OCCUR);
  } else if (isPoint) {
    beforeText = beforeChalk.hex(stateColor)(DOT);
  } else if (
    (noStateChange && !isNegativeDur) ||
    (!noStateChange && isNegativeDur)
  ) {
    beforeText = beforeChalk.bold.hex(
      getContrastTextColor(
        getStateColor({ field, state: lastState }),
        "warning",
      ),
    )(WARNING);
  } else if (lastState === null) {
    beforeText = beforeChalk(NULL);
  } else if (Array.isArray(lastState)) {
    beforeText = beforeChalk(SHADING);
  } else {
    beforeText = beforeChalk(" ");
  }

  const currentChalk =
    isNegativeDur || isPoint
      ? chalk.hex(stateColor)
      : stateChalk({ field, state });
  let currentText: string;
  if (state === undefined) {
    currentText = "";
  } else if (state === null) {
    currentText = currentChalk(NULL);
  } else if (Array.isArray(state)) {
    currentText = !isNegativeDur
      ? state
          .map((substate) =>
            stateChalk({ field, state: substate })(`${substate},`),
          )
          .join("")
          .slice(0, -1)
      : stateChalk({ field, state: false })(state.join(","));
  } else if (isStateObject(state)) {
    const stateId = (state as StateObject).id;
    currentText =
      stateId !== undefined ? currentChalk(`{${stateId}}`) : currentChalk("{}");
  } else if (state === true) {
    currentText = currentChalk("start");
  } else if (state === false) {
    currentText = currentChalk("end");
  } else {
    currentText = currentChalk(state);
  }

  let afterText: string;
  if (isNonOccur || isPoint) {
    afterText = "";
  } else if (isFalseState) {
    afterText = "";
  } else if (dur !== undefined) {
    afterText = beforeChalk(" ");
  } else if (Array.isArray(state)) {
    afterText = currentChalk(SHADING);
  } else {
    afterText = currentChalk(" ");
  }

  return beforeText + currentText + afterText;
}

function formatDuration(dur?: string | undefined | null): string | undefined {
  const duration = Duration.fromISO(dur || "");
  if (!duration.isValid) {
    return undefined;
  }
  return duration.toFormat("m'm'");
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
    field: formatField(data.field),
    state: formatState(data),
    dur: formatDuration(data.dur),
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
  const infoLine = [
    formatted.time.occur,
    formatted.field,
    formatted.state,
    formatted.dur,
  ]
    .filter(Boolean)
    .join(" ");
  if (infoLine !== "") {
    console.log(infoLine);
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
      : ((outputArgs.showAll ? Show.All : outputArgs.show) ??
        (outputArgs.formatString ? Show.Format : Show.None));
  return { show, formatString: outputArgs.formatString };
}

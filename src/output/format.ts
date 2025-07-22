import {
  DatumData,
  EitherPayload,
} from "../documentControl/DatumDocument";
import chalk, { Chalk } from "chalk";
import stringify from "string.ify";
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

export enum ACTIONS {
  Create = "CREATE",
  Delete = "DELETE",
  Exists = "EXISTS",
  Update = "UPDATE",
  OWrite = "OWRITE",
  Rename = "RENAME",
  NoDiff = "NODIFF",
  Failed = "FAILED",
}

export const ACTION_CHALK: { [key in ACTIONS]: Chalk } = {
  [ACTIONS.Create]: chalk.green,
  [ACTIONS.Delete]: chalk.red,
  [ACTIONS.Exists]: chalk.yellow,
  [ACTIONS.Update]: chalk.cyan,
  [ACTIONS.OWrite]: chalk.blue,
  [ACTIONS.Rename]: chalk.cyan,
  [ACTIONS.NoDiff]: chalk.hex("#ffa500"),
  [ACTIONS.Failed]: chalk.red,
};

export type AllTimes = {
  hybrid?: string;
  occur?: string;
  modify?: string;
  create?: string;
  none: undefined;
};

export function formatAllTimes(doc: EitherPayload): AllTimes {
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

export function formatField(field?: string, doc?: EitherPayload): string | undefined {
  if (field === undefined) {
    return undefined;
  }

  // If field contains % syntax and we have document data, interpolate it
  if (field.includes("%") && doc) {
    const { data, meta } = pullOutData(doc);
    const interpolatedField = interpolateFields({
      data,
      meta,
      format: field,
    });
    return fieldChalk({ field: interpolatedField })(interpolatedField);
  }

  return fieldChalk({ field })(field);
}

const DOT = "●" as const;
const NON_OCCUR = "¢" as const;
const SHADING = "▞" as const;
const WARNING = "!" as const;
const NULL = "∅" as const;

export function formatState(data: DatumData): string | undefined {
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
          .map((substate, i) => {
            const substateText = isStateObject(substate)
              ? `{${substate.id ?? ""}}`
              : substate;
            return stateChalk({ field, state: substate })(
              `${i > 0 ? "," : ""}${substateText}`,
            );
          })
          .join("")
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
  } else if (dur !== undefined) {
    afterText = beforeChalk(" ");
  } else if (isFalseState) {
    afterText = "";
  } else if (Array.isArray(state)) {
    afterText = currentChalk(SHADING);
  } else {
    afterText = currentChalk(" ");
  }

  return beforeText + currentText + afterText;
}

export function formatDuration(dur?: string | undefined | null): string | undefined {
  const duration = Duration.fromISO(dur || "");
  if (!duration.isValid) {
    return undefined;
  }
  return duration.toFormat("m'm'");
}

export function formattedNonRedundantData(doc: EitherPayload): string | undefined {
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

export function formattedDoc(doc: EitherPayload): string {
  return stringify(doc);
}

export type ExtractedAndFormatted = {
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
    field: formatField(data.field, doc),
    state: formatState(data),
    dur: formatDuration(data.dur),
  };
}

export function actionId(action: ACTIONS, id: string, humanId?: string): string {
  const color = ACTION_CHALK[action];
  const actionText = color.inverse(` ${action} `);
  const quickId = humanId ? ` (${humanId.slice(0, 5)})` : "";
  return actionText + color(id) + quickId;
}
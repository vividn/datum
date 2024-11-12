import chalk, { Chalk } from "chalk";
import { DatumState } from "../state/normalizeState";
import { getContrastTextColor } from "../utils/colorUtils";
import { md5Color } from "../utils/md5Color";
import { getFieldSpec } from "./mySpecs";
import { simplifyState } from "../state/simplifyState";

export function getFieldColor(field?: string): string {
  if (field === undefined) {
    return "white";
  }
  const spec = getFieldSpec(field);
  const fieldColor = spec.color ?? md5Color(field);
  return fieldColor;
}

export function fieldChalk({ field }: { field?: string }): Chalk {
  const color = getFieldColor(field);
  const fg = getContrastTextColor(color);
  return chalk.bgHex(color).hex(fg);
}

export function getStateColor({
  state: rawState,
  field,
}: {
  state?: DatumState;
  field?: string;
}): string {
  if (rawState === undefined) {
    return getFieldColor(field);
  }
  const state = simplifyState(rawState);
  if (state === true) {
    return getFieldColor(field);
  }
  if (state === false) {
    return "#000000";
  }
  if (state === null) {
    return "#888888";
  }
  if (Array.isArray(state)) {
    return getStateColor({ state: state[0], field });
  }
  const spec = getFieldSpec(field);
  const color = spec.states?.[state]?.color ?? md5Color(state);
  return color;
}

export function stateChalk({
  state: rawState,
  field,
}: {
  state?: DatumState;
  field?: string;
}): Chalk {
  const fieldColor = getFieldColor(field);
  if (rawState === undefined) {
    return chalk.hex(fieldColor);
  }
  const state = simplifyState(rawState);
  if (state === true) {
    return fieldChalk({ field: field });
  }
  if (state === false || state === undefined) {
    return chalk;
  }
  if (state === null) {
    return chalk.bgHex("#888888").hex("#666666");
  }
  if (Array.isArray(state)) {
    return chalk
      .bgHex(getStateColor({ state: state[0], field }))
      .hex(getStateColor({ state: state[1], field }));
  }
  const color = getStateColor({ state, field });
  const fg = getContrastTextColor(color);
  return chalk.bgHex(color).hex(fg);
}

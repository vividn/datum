import chalk from "chalk";
import { SingleState } from "../state/normalizeState";
import { getContrastTextColor } from "../utils/colorUtils";
import { md5Color } from "../utils/md5Color";
import { FIELD_SPECS } from "./mySpecs";
import { SimpleSingleState } from "../state/simplifyState";

export function getFieldColor(field: string): string {
  const spec = FIELD_SPECS[field] ?? {};
  const fieldColor = spec.color ?? md5Color(field);
  return fieldColor;
}

export function fieldChalk({
  field,
}: {
  field: string;
}): (text: string) => string {
  const color = getFieldColor(field);
  const fg = getContrastTextColor(color);
  return chalk.bgHex(color).hex(fg);
}

export function getStateColor({
  state,
  field,
}: {
  state: string | boolean | null;
  field?: string;
}): string {
  if (state === true) {
    return getFieldColor(field ?? "");
  }
  if (state === false) {
    return "#000000";
  }
  if (state === null) {
    return "#888888";
  }
  const spec = FIELD_SPECS[field ?? ""] ?? {};
  const color = spec.states?.[state]?.color ?? md5Color(state);
  return color;
}

export function stateChalk({
  state,
  field,
}: {
  state: string | boolean | null;
  field?: string;
}): (text: string) => string {
  const fieldColor = getFieldColor(field ?? "");
  if (state === true) {
    return fieldChalk({ field: field ?? "" });
  }
  if (state === false) {
    return chalk.hex(fieldColor);
  }
  if (state === null) {
    return chalk.bgHex("#888888").hex("#666666")
  }
  const color = getStateColor({ state, field });
  const fg = getContrastTextColor(color);
  return chalk.bgHex(color).hex(fg);
})

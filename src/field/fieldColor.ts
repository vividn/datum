import chalk from "chalk";
import { SingleState } from "../state/normalizeState";
import { getContrastTextColor } from "../utils/colorUtils";
import { md5Color } from "../utils/md5Color";
import { FIELD_SPECS } from "./mySpecs";
import { SimpleSingleState } from "../state/simplifyState";

export function fieldColor(field: string): string {
  const spec = FIELD_SPECS[field] ?? {};
  const fieldColor = spec.color ?? md5Color(field);
  return fieldColor;
}

export function fieldChalk({
  field,
}: {
  field: string;
}): (text: string) => string {
  const color = fieldColor(field);
  const fg = getContrastTextColor(color);
  return chalk.bgHex(color).hex(fg);
}

export function stateColor({
  state,
  field,
}: {
  state: string;
  field?: string;
}): string {
  const spec = FIELD_SPECS[field ?? ""] ?? {};
  const color = spec.states?.[state]?.color ?? md5Color(state);
  return color;
}

export function stateChalk({
  state,
  field,
}: {
  state: SingleState;
  field?: string;
}): (text: string) => string {
  const color = stateColor({ state, field });
  const fg = getContrastTextColor(color);
  return chalk.bgHex(color).hex(fg);
})

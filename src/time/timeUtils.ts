import { DateTime } from "luxon";

export type isoDatetime = string;
export type isoDate = string;

export function isIsoDateOrTime(str: string): str is isoDate | isoDatetime {
  return DateTime.fromISO(str).isValid;
}

export const now = DateTime.local;
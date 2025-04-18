import { isIsoDateOrTime, isoDuration } from "../time/timeUtils";
import { JsonObject, WithRequired } from "../utils/utilityTypes";

import { DatumState } from "../state/normalizeState";
import { DatumTime } from "../time/datumTime";

export type DatumData<T = unknown> = JsonObject & {
  state?: DatumState;
  lastState?: DatumState;
  occurTime?: DatumTime;
  dur?: isoDuration | null;
  field?: string;
} & T;

export type OccurredData<T = unknown> = WithRequired<DatumData<T>, "occurTime">;

export function isOccurredData(
  data: DatumData | OccurredData,
): data is OccurredData {
  const occurTime = data.occurTime;
  return occurTime !== undefined && isIsoDateOrTime(occurTime.utc);
}

export type DatumMetadata = {
  createTime?: DatumTime;
  modifyTime?: DatumTime; //TODO: turn into an array of times
  idStructure?: string;
  fieldStructure?: string;
  humanId?: string;
  // [key: string]: any;
};

export type DatumPayload<T = unknown> = {
  _id?: string;
  _rev?: string;
  data: DatumData<T>;
  meta: DatumMetadata;
};

export type DatumIdPayload<T = unknown> = DatumPayload<T> & {
  _id: string;
};

export type DatumDocument<T = unknown> = DatumPayload<T> & {
  _id: string;
  _rev: string;
};

export type DataOnlyPayload<T = unknown> = {
  _id?: string;
  _rev?: string;
} & DatumData<T>;

export type DataOnlyIdPayload<T = unknown> = DataOnlyPayload<T> & {
  _id: string;
};

export type DataOnlyDocument<T = unknown> = DataOnlyPayload<T> & {
  _id: string;
  _rev: string;
};

export type EitherPayload<T = unknown> = DatumPayload<T> | DataOnlyPayload<T>;
export type EitherIdPayload<T = unknown> =
  | DatumIdPayload<T>
  | DataOnlyIdPayload<T>;
export type EitherDocument<T = unknown> =
  | DatumDocument<T>
  | DataOnlyDocument<T>;

export function isDatumDocument(
  doc: DatumDocument | DataOnlyDocument,
): doc is DatumDocument {
  return (
    (doc as DatumDocument).data !== undefined &&
    (doc as DatumDocument).meta !== undefined
  );
}

export function isDatumPayload(
  payload: DatumPayload | DataOnlyPayload,
): payload is DatumPayload {
  return (
    (payload as DatumPayload).data !== undefined &&
    (payload as DatumPayload).meta !== undefined
  );
}

export type ExtractDataType<T> = T extends DatumDocument<infer U> ? U : never;

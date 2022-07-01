import {
  isIsoDateOrTime,
  isoDate,
  isoDatetime,
  isoDuration,
} from "../time/timeUtils";
import { WithRequired } from "../utils/utilityTypes";

export type GenericData<T = unknown> = T & {
  [key: string]: any;
};

export type DatumData<T = unknown> = GenericData<T> & {
  occurTime?: isoDatetime | isoDate;
  occurUtcOffset?: number;
  dur?: "start" | "end" | isoDuration;
};

export type OccurredData<T = unknown> = WithRequired<
  DatumData<T>,
  "occurTime" | "occurUtcOffset"
>;

export function isOccurredData(
  data: DatumData | OccurredData
): data is OccurredData {
  const occurTime = data.occurTime;
  return occurTime !== undefined && isIsoDateOrTime(occurTime);
}

export type DatumMetadata = {
  utcOffset?: number;
  createTime?: isoDatetime;
  modifyTime?: isoDatetime; //TODO: turn into an array of times
  idStructure?: string;
  random?: number;
  humanId?: string;
  // [key: string]: any;
};

export type DatumPayload<T = unknown> = {
  _id?: string;
  _rev?: string;
  data: DatumData<T>;
  meta: DatumMetadata;
};

export type DatumDocument<T = unknown> = DatumPayload<T> & {
  _id: string;
  _rev: string;
};

export type DataOnlyPayload<T = unknown> = {
  _id?: string;
  _rev?: string;
} & DatumData<T>;

export type DataOnlyDocument<T = unknown> = DataOnlyPayload<T> & {
  _id: string;
  _rev: string;
};

export type EitherPayload<T = unknown> = DatumPayload<T> | DataOnlyPayload<T>;
export type EitherDocument<T = unknown> =
  | DatumDocument<T>
  | DataOnlyDocument<T>;

export function isDatumDocument(
  doc: DatumDocument | DataOnlyDocument
): doc is DatumDocument {
  return (
    (doc as DatumDocument).data !== undefined &&
    (doc as DatumDocument).meta !== undefined
  );
}

export function isDatumPayload(
  payload: DatumPayload | DataOnlyPayload
): payload is DatumPayload {
  return (
    (payload as DatumPayload).data !== undefined &&
    (payload as DatumPayload).meta !== undefined
  );
}

import { isIsoDateOrTime, isoDate, isoDatetime } from "../time/timeUtils";

export type Occurance = {
  occurTime: isoDatetime | isoDate;
  occurUtcOffset?: number
};

export type GenericData<T = any> = T & {
  [key: string]: any;
};
export type OccurredData<T = any> = Occurance & T & {
  [key: string]: any;
}

export type DatumData<T = any> = GenericData<T> | OccurredData<T> ;

export function isOccurredData(data: DatumData): data is OccurredData {
  return (
    (data as OccurredData).occurTime !== undefined &&
    isIsoDateOrTime(data.occurTime)
  );
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

export type DatumPayload<T = any> = {
  _id?: string;
  _rev?: string;
  data: DatumData<T>;
  meta: DatumMetadata;
};

export type DatumDocument<T = any> = DatumPayload<T> & {
  _id: string;
  _rev: string;
};

export type DataOnlyPayload<T = any> = {
  _id?: string;
  _rev?: string;
} & DatumData<T>;

export type DataOnlyDocument<T = any> = DataOnlyPayload<T> & {
  _id: string;
  _rev: string;
};

export type EitherPayload<T = any> = DatumPayload<T> | DataOnlyPayload<T>;
export type EitherDocument<T = any> = DatumDocument<T> | DataOnlyDocument<T>;

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

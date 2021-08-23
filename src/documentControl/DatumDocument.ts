import { isIsoDateOrTime, isoDate, isoDatetime } from "../timings";

export type occurredData = {
  occurTime?: isoDatetime | isoDate;
  [key: string]: any;
};

export type GenericData = {
  [key: string]: any;
};

export type DatumData = occurredData | GenericData;

export function isOccurredData(data: DatumData): data is occurredData {
  return (
    (data as occurredData).occurTime !== undefined &&
    isIsoDateOrTime(data.occurTime)
  );
}

export type DatumMetadata = {
  occurTime?: isoDate | isoDatetime;
  utcOffset?: number;
  createTime?: isoDatetime;
  modifyTime?: isoDatetime;
  idStructure?: string;
  random?: number;
  humanId?: string;
  // [key: string]: any;
};

export type DatumPayload = {
  _id?: string;
  _rev?: string;
  data: DatumData;
  meta: DatumMetadata;
};

export type DatumDocument = DatumPayload & {
  _id: string;
  _rev: string;
};

export type DataOnlyPayload = {
  _id?: string;
  _rev?: string;
} & DatumData;

export type DataOnlyDocument = DataOnlyPayload & {
  _id: string;
  _rev: string;
};

export type EitherPayload = DatumPayload | DataOnlyPayload;
export type EitherDocument = DatumDocument | DataOnlyDocument;

export const isDatumDocument = (
  doc: DatumDocument | DataOnlyDocument
): doc is DatumDocument => {
  return (
    (doc as DatumDocument).data !== undefined &&
    (doc as DatumDocument).meta !== undefined
  );
};

export const isDatumPayload = (
  payload: DatumPayload | DataOnlyPayload
): payload is DatumPayload => {
  return (
    (payload as DatumPayload).data !== undefined &&
    (payload as DatumPayload).meta !== undefined
  );
};

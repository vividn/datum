import { isoDate, isoDatetime } from "../timings";

export type DatumData = {
  [key: string]: any;
};

export type DatumMetadata = {
  occurTime?: isoDate | isoDatetime;
  utcOffset: number;
  createTime: isoDatetime;
  modifyTime: isoDatetime;
  idStructure?: string;
  random: number;
  humanId: string;
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

export const isDatumDocument = (
  doc: DatumDocument | DataOnlyDocument
): doc is DatumDocument => {
  return (
    (doc as DatumDocument).data !== undefined &&
    (doc as DatumDocument).meta !== undefined
  );
};

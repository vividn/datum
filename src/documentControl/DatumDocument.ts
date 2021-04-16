import { GenericObject } from "../GenericObject";

export type DatumDocument = {
  _id: string;
  _rev: string;
  meta?: {
    occurTime?: string;
    createTime?: string;
    modifyTime?: string;
    utcOffset?: number;
    idStructure?: string;
  };
} & GenericObject;

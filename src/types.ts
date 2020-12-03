export type CouchDocument = {
  _id: string;
  _rev: string;
  meta?: {
    occurTime?: string;
    createTime?: string;
    modifyTime?: string;
    utcOffset?: number;
    idStructure?: string;
  }
} & GenericObject

export type GenericObject = {
  [key: string]: any;
}
import { EitherDocument } from "../documentControl/DatumDocument";

export type WithRequired<T, K extends keyof T> = T & { [P in K]-?: T[P] };

export type ViewRow<KeyType, ValueType, DocumentType = EitherDocument> = {
  id: string;
  key: KeyType;
  value: ValueType;
  doc?: DocumentType;
};

export type JsonType =
  | number
  | string
  | boolean
  | null
  | JsonType[]
  | { [key: string]: JsonType };

export type GenericType =
  | JsonType
  | undefined
  | GenericType[]
  | { [key: string]: GenericType };

export type JsonObject = { [key: string]: JsonType };

export type GenericObject = { [key: string]: GenericType };

export type QueryOptions = PouchDB.Query.Options<any, any>;

export function isJsonObject(obj: any): obj is JsonObject {
  return typeof obj === "object" && !Array.isArray(obj) && obj !== null;
}

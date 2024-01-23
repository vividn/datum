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

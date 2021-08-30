import { DocumentScope, ViewDocument } from "nano";
import { EitherDocument } from "../documentControl/DatumDocument";

type GetViewDocType = {};

export async function getViewDoc() {}

export function asViewDb<T>(
  db: DocumentScope<T>
): DocumentScope<ViewDocument<T>> {
  return db as unknown as DocumentScope<ViewDocument<T>>;
}

export type DatumView<D extends EitherDocument> = {
  name: string;
  map: MapFunction<D>;
  reduce?: ReduceFunction | { [viewName: string]: ReduceFunction };
};

export type StringifiedDatumView = {
  name: string;
  map: string;
  reduce?: string | { [viewName: string]: string };
};

export type ReduceFunction =
  | ((keysAndDocIds: [any, string][], values: [], rereduce: boolean) => any)
  | "_sum"
  | "_stats"
  | "_count"
  | "_approx_count_distinct";
export type MapFunction<D extends EitherDocument> = (doc: D) => void;

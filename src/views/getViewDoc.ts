import { DocumentScope } from "nano";
import { DatumMetadata, EitherDocument, EitherPayload } from "../documentControl/DatumDocument";

export function asViewDb<D extends EitherDocument>(
  db: DocumentScope<D>
): DocumentScope<ViewDocument<D>> {
  return db as unknown as DocumentScope<ViewDocument<D>>;
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

export type ViewPayload<D extends EitherDocument> = {
  _id: string;
  _rev?: string;
  views: {
    [viewName: string]: {
      map: MapFunction<D> | string;
      reduce?: ReduceFunction | string;
    };
  };
  meta?: DatumMetadata
};

type EitherViewPayload = ViewPayload<EitherDocument>;

export type ViewDocument<D extends EitherDocument> = ViewPayload<D> & {
  _rev: string;
}

export type EitherViewDocument = ViewDocument<EitherDocument>;

export function isViewPayload (payload: EitherPayload | EitherViewPayload): payload is EitherViewPayload {
  return !!(payload._id && payload._id.startsWith("_design/") && (payload as EitherViewPayload).views);
}

export function isViewDocument(doc: EitherDocument | EitherViewDocument): doc is EitherViewDocument {
  return !!(doc._id.startsWith("_design") && (doc as EitherViewDocument).views);
}
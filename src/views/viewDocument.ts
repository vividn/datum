import {
  DatumMetadata,
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";

// export function asViewDb<D extends EitherPayload>(
//   db: DocumentScope<D>
// ): DocumentScope<ViewPayload<D>> {
//   return db as unknown as DocumentScope<ViewPayload<D>>;
// }

export type DatumView<D extends EitherDocument = EitherDocument> = {
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
export type MapFunction<D extends EitherDocument = EitherDocument> = (
  doc: D
) => void;

export type ViewPayload = {
  _id: string;
  _rev?: string;
  views: {
    [viewName: string]: {
      map: string;
      reduce?: string;
    };
  };
  meta?: DatumMetadata;
};

export type ViewDocument = ViewPayload & {
  _rev: string;
};

export function isViewPayload(
  payload: EitherPayload | ViewPayload
): payload is ViewPayload {
  return !!(
    payload._id &&
    payload._id.startsWith("_design/") &&
    (payload as ViewPayload).views
  );
}

export function isViewDocument(
  doc: EitherDocument | ViewDocument
): doc is ViewDocument {
  return !!(doc._id.startsWith("_design") && (doc as ViewDocument).views);
}

export function datumViewToViewPayload(datumView: DatumView): ViewPayload {}

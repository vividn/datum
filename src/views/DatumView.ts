import {
  DatumMetadata,
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { DocumentScope } from "nano";

export function asViewDb(db: DocumentScope<any>): DocumentScope<ViewPayload> {
  return db as unknown as DocumentScope<ViewPayload>;
}

// TODO: Expand types of DatumView to include information about format of map rows and reduce values

export type DatumView<
  DocType extends EitherDocument = EitherDocument,
  MapKey = unknown,
  MapValue = unknown,
  NamedReduceValues extends Record<string, any> = {
    default: unknown;
  }
> = {
  name: string;
  emit: (key: MapKey, value: MapValue) => void;
  map: MapFunction<DocType>;
  reduce?: NamedReduceFunctions<MapKey, MapKey, NamedReduceValues>;
  options?: ViewOptions;
};

type BuiltInReduce = "_sum" | "_stats" | "_count" | "_approx_count_distinct";

export type StringifiedDatumView = {
  name: string;
  map: string;
  reduce?: Record<string, string>;
  options?: ViewOptions;
};

export type NamedReduceFunctions<
  MapKey,
  MapValue,
  NamedReduceValues extends Record<string, any>
> = {
  [T in keyof NamedReduceValues]:
    | ReduceFunction<MapKey, MapValue, NamedReduceValues[T]>
    | BuiltInReduce;
};

export type ReduceFunction<
  MapKey = unknown,
  MapValue = unknown,
  ReduceValue = unknown
> =
  | ((
      keysAndDocIds: [MapKey, string][],
      values: MapValue[],
      rereduce: false
    ) => ReduceValue)
  | ((
      keysAndDocIds: [MapKey, null][],
      values: ReduceValue[],
      rereduce: true
    ) => ReduceValue)
  | BuiltInReduce;

export type MapFunction<D extends EitherDocument = EitherDocument> = (
  doc: D
) => void;

type ViewOptions = {
  collation?: "raw";
};

type ViewPayloadViews = {
  [viewName: string]: {
    map: string;
    reduce?: string;
    options?: ViewOptions;
  };
};

export type ViewPayload = {
  _id: string;
  _rev?: string;
  views: ViewPayloadViews;
  meta?: DatumMetadata;
};

export type DataOrDesignPayload = ViewPayload | EitherPayload;

export type ViewDocument = ViewPayload & {
  _rev: string;
};

export type DataOrDesignDocument = ViewDocument | EitherDocument;

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

export function datumViewToViewPayload(
  datumView:
    | DatumView<any, any, any, Record<string, any>>
    | StringifiedDatumView
): ViewPayload {
  const views: ViewPayloadViews = {};
  const mapStr = datumView.map.toString();
  const datumReduce = datumView.reduce as
    | NamedReduceFunctions<any, any, Record<string, any>>
    | Record<string, string>;
  const options = datumView.options;

  if (datumReduce === undefined) {
    views.default = {
      map: mapStr,
      options,
    };
  } else {
    for (const reduceName in datumReduce) {
      views[reduceName] = {
        map: mapStr,
        reduce: datumReduce[reduceName].toString(),
        options,
      };
    }
  }

  return {
    _id: `_design/${datumView.name}`,
    views: views,
    meta: {},
  };
}

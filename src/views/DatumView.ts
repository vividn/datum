import isPlainObject from "lodash.isplainobject";
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
  ReduceValue = MapValue
> = {
  name: string;
  emit: (key: MapKey, value: MapValue) => void;
  map: MapFunction<DocType>;
  reduce?: ReduceFunction<MapKey, MapValue, ReduceValue> | MultiReduceFunction;
  options?: ViewOptions;
};

type MultiReduceFunction = {
  [viewName: string]: ReduceFunction;
};

type BuiltInReduce = "_sum" | "_stats" | "_count" | "_approx_count_distinct";

export type StringifiedDatumView = {
  name: string;
  map: string;
  reduce?: string | MultiStringReduceFunction;
  options?: ViewOptions;
};

type MultiStringReduceFunction = { [viewName: string]: string };

function isMultiReduce(
  reduce:
    | undefined
    | ReduceFunction
    | string
    | MultiReduceFunction
    | MultiStringReduceFunction
): reduce is MultiReduceFunction | MultiStringReduceFunction {
  return isPlainObject(reduce);
}

export type ReduceFunction<
  MapKey = unknown,
  MapValue = unknown,
  ReduceValue = unknown
> =
  | ((
      keysAndDocIds: [MapKey, string][],
      values: MapValue[],
      rereduce: boolean
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
  datumView: DatumView | StringifiedDatumView
): ViewPayload {
  const views: ViewPayloadViews = {};
  const mapStr = datumView.map.toString();
  const datumReduce = datumView.reduce;
  const options = datumView.options;

  if (isMultiReduce(datumReduce)) {
    for (const reduceName in datumReduce) {
      views[reduceName] = {
        map: mapStr,
        reduce: datumReduce[reduceName].toString(),
        options,
      };
    }
    if (views.default === undefined) {
      views.default = {
        map: mapStr,
        options,
      };
    }
  } else {
    views.default =
      datumReduce === undefined
        ? {
            map: mapStr,
            options,
          }
        : {
            map: mapStr,
            reduce: datumReduce.toString(),
            options,
          };
  }

  return {
    _id: `_design/${datumView.name}`,
    views: views,
    meta: {},
  };
}

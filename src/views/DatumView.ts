import {
  DatumMetadata,
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { MyError } from "../errors";

export function asViewDb(
  db: PouchDB.Database<any>,
): PouchDB.Database<ViewPayload> {
  return db as unknown as PouchDB.Database<ViewPayload>;
}

export type DatumView<
  DocType extends EitherDocument = EitherDocument,
  MapKey = unknown,
  MapValue = unknown,
  ReduceValue = unknown,
  NamedReduceValues extends Record<string, any> | undefined = undefined,
> = {
  name: string;
  emit: (key: MapKey, value: MapValue) => void;
  map: MapFunction<DocType>;
  reduce?: ReduceFunction<MapKey, MapValue, ReduceValue> | BuiltInReduce;
  namedReduce?: NamedReduceFunctions<MapKey, MapValue, NamedReduceValues>;
  options?: ViewOptions;
};

type BuiltInReduce = "_sum" | "_stats" | "_count" | "_approx_count_distinct";

export type StringifiedDatumView = {
  name: string;
  map: string;
  reduce?: string;
  namedReduce?: Record<string, string>;
  options?: ViewOptions;
};

export type NamedReduceFunctions<
  MapKey,
  MapValue,
  NamedReduceValues extends Record<string, any> | undefined,
> = {
  [T in keyof NamedReduceValues]:
    | ReduceFunction<MapKey, MapValue, NamedReduceValues[T]>
    | BuiltInReduce;
};

type FirstReduceArgs<MapKey, MapValue, _ReduceValue> = [
  keysAndDocIds: [MapKey, string][],
  values: MapValue[],
  rereduce: false,
];
type ReReduceArgs<MapKey, _MapValue, ReduceValue> = [
  keysAndDocIds: [MapKey, null][],
  values: ReduceValue[],
  rereduce: true,
];
export type ReduceFunction<
  MapKey = unknown,
  MapValue = unknown,
  ReduceValue = unknown,
> =
  | ((
      ...args:
        | FirstReduceArgs<MapKey, MapValue, ReduceValue>
        | ReReduceArgs<MapKey, MapValue, ReduceValue>
    ) => ReduceValue)
  | BuiltInReduce;

export type MapFunction<D extends EitherDocument = EitherDocument> = (
  doc: D,
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
  payload: EitherPayload | ViewPayload,
): payload is ViewPayload {
  return !!(
    payload._id &&
    payload._id.startsWith("_design/") &&
    (payload as ViewPayload).views
  );
}

export function isViewDocument(
  doc: EitherDocument | ViewDocument,
): doc is ViewDocument {
  return !!(doc._id.startsWith("_design") && (doc as ViewDocument).views);
}

export class ConflictingReduceError extends MyError {
  constructor(reduce_name?: unknown) {
    super(
      `Named reduce function ${reduce_name} conflicts with the default. Please use reduce: directly or rename the function.`,
    );
    Object.setPrototypeOf(this, ConflictingReduceError.prototype);
  }
}

export function datumViewToViewPayload(
  datumView:
    | DatumView<any, any, any, any, Record<string, any> | undefined>
    | StringifiedDatumView,
): ViewPayload {
  const views: ViewPayloadViews = {};
  const name = datumView.name;
  const mapStr = datumView.map.toString();
  const options = datumView.options;
  const defaultReduce = datumView.reduce;
  const namedReduce = datumView.namedReduce;

  if (datumView.reduce && datumView.namedReduce?.[name]) {
    throw new ConflictingReduceError(name);
  }

  if (defaultReduce) {
    views[name] = {
      map: mapStr,
      reduce: defaultReduce.toString(),
      options,
    };
  }

  if (namedReduce) {
    for (const reduceName in datumView.namedReduce) {
      views[reduceName] = {
        map: mapStr,
        reduce: namedReduce[reduceName].toString(),
        options,
      };
    }
  }

  // set default view with just map function if no default reduce exists
  if (!views[name]) {
    views[name] = {
      map: mapStr,
      options,
    };
  }

  return {
    _id: `_design/${datumView.name}`,
    views: views,
    meta: {},
  };
}

export type MapRow<T> =
  T extends DatumView<any, infer K, infer V, any, any>
    ? {
        id: string;
        key: K;
        value: V;
      }
    : never;

export type ReduceRow<T> =
  T extends DatumView<any, infer K, any, infer R, any>
    ? {
        id: null;
        key: K;
        value: R;
      }
    : never;

export type NamedReduceRow<T, Name extends string> =
  T extends DatumView<any, infer K, any, any, infer NR>
    ? NR extends Record<Name, infer V>
      ? {
          id: null;
          key: K;
          value: V;
        }
      : never
    : never;

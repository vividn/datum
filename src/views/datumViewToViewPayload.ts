import { DatumView, StringifiedDatumView, ViewPayload, ViewPayloadViews, ConflictingReduceError } from "./DatumView";


export function datumViewToViewPayload(
  datumView: DatumView<any, any, any, any, Record<string, any> | undefined> |
    StringifiedDatumView
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

import { DatumData, EitherDocument } from "../../documentControl/DatumDocument";
import { DatumState } from "../../state/normalizeState";
import { isoDateOrTime } from "../../time/timeUtils";
import { DatumView, MapRow } from "../DatumView";
import { _emit } from "../emit";

type DocType = EitherDocument;
type MapKey = [string, isoDateOrTime];
type MapValue = DatumState;
type ReduceValues = number;

export type PointDataRow = MapRow<typeof pointDataView>;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const pointDataView: DatumView<DocType, MapKey, MapValue, ReduceValues> =
  {
    name: "point_data",
    map: (doc) => {
      let data: DatumData;
      if (doc.data && doc.meta) {
        data = doc.data as DatumData;
      } else {
        data = doc;
      }
      const occurTime = data.occurTime;
      const field = data.field;
      if (!occurTime || !field) {
        return;
      }
      const duration =
        data.dur !== undefined
          ? data.dur
          : data.state === undefined
            ? null
            : undefined;
      const state: DatumState = data.state ?? true;
      if (duration === null) {
        emit([field, occurTime.utc], state);
      }
    },
    reduce: "_count",
  };

import { _emit } from "../../../src/views/emit";
import { FinanceDoc } from "./balance";
import { DatumView } from "../../../src/views/DatumView";
import { isoDateOrTime } from "../../../src/time/timeUtils";
import { DatumTime } from "../../../src/time/datumTime";

type DocType = FinanceDoc;
type MapKey = [string, string, isoDateOrTime?];
type MapValue = number;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const equalityView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "equality",
  emit,
  map: (doc) => {
    function dtTransform(
      time: string | DatumTime | undefined,
    ): DatumTime | undefined {
      // TODO: Remove this once all documents are migrated to new format
      if (typeof time === "string") {
        return { utc: time };
      }
      return time;
    }
    const data = doc.data;
    const occurDatumTime = dtTransform(
      data.effectiveTime || data.effectiveDate || data.occurTime,
    );
    if (occurDatumTime === undefined) {
      return;
    }
    const occurTime = occurDatumTime.utc;
    if (data.type === "eq") {
      emit([data.acc, data.curr, occurTime], data.bal);
    }
  },
  reduce: "_count",
};

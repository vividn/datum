import { _emit } from "../../../src/views/emit";
import { FinanceDoc } from "./balance";
import { DatumView } from "../../../src/views/DatumView";
import { isoDateOrTime } from "../../../src/time/timeUtils";

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
    const data = doc.data;
    const occurTime = data.effectiveTime || data.effectiveDate || data.occurTime;
    if (data.type === "eq") {
      emit([data.acc, data.curr, occurTime], data.bal);
    }
  },
  reduce: "_count",
};

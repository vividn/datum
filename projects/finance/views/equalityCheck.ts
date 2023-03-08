import { _emit } from "../../../src/views/emit";
import { FinanceDoc } from "./balance";
import { DatumView } from "../../../src/views/DatumView";

function emit(key: unknown, value: unknown): void {
  _emit(key, value);
}

export const equalityView: DatumView<FinanceDoc> = {
  name: "equality",
  emit,
  map: (doc) => {
    const data = doc.data;
    if (data.type === "eq") {
      emit([data.acc, data.curr, data.occurTime], data.bal);
    }
  },
  reduce: {
    default: "_count",
  },
};

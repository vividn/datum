import { _emit } from "../../../src/views/emit";
import { FinanceDoc } from "./balance";
import { DatumView } from "../../../src/views/viewDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const equalityView: DatumView<FinanceDoc> = {
  name: "equality",
  map: (doc) => {
    const data = doc.data;
    if (data.type === "eq") {
      emit([data.acc, data.curr, data.occurTime], data.bal);
    }
  },
  reduce: "_count",
};

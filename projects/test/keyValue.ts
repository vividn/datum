import { _emit } from "../../src/views/emit";
import { DatumView } from "../../src/views/viewDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const keyValueView: DatumView = {
  name: "update_seq_view",
  map: (doc) => {
    if (doc.data.key && doc.data.value) {
      emit(doc.data.key, doc.data.value);
    }
  }
};
import { _emit } from "../../../../views/emit";
import { DatumView } from "../../../../views/viewDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const keyValueView: DatumView = {
  name: "key_value_view",
  map: (doc) => {
    if (doc.data.key && doc.data.value) {
      emit(doc.data.key, doc.data.value);
    }
  },
};

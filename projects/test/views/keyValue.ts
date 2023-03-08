import { _emit } from "../../../src/views/emit";
import { DatumView } from "../../../src/views/DatumView";

function emit(key: unknown, value: unknown): void {
  _emit(key, value);
}

export const keyValueView: DatumView<any, any, any, undefined> = {
  name: "key_value_view",
  emit,
  map: (doc) => {
    if (doc.data.key && doc.data.value) {
      emit(doc.data.key, doc.data.value);
    }
  },
};

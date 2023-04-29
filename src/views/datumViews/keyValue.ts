import { _emit } from "../emit";
import { DatumView } from "../DatumView";

function emit(key: unknown, value: unknown): void {
  _emit(key, value);
}

export const keyValueView: DatumView<any, any, any, undefined> = {
  name: "key_value",
  emit,
  map: (doc) => {
    if (doc.data.key && doc.data.value) {
      emit(doc.data.key, doc.data.value);
    }
  },
};
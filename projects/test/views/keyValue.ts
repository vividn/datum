import { _emit } from "../../../src/views/emit.js";
import { DatumView } from "../../../src/views/DatumView.js";
import { DatumDocument } from "../../../src/documentControl/DatumDocument.js";

function emit(key: unknown, value: unknown): void {
  _emit(key, value);
}

export const keyValueView: DatumView<DatumDocument> = {
  name: "key_value_view",
  map: (doc) => {
    if (doc.data.key && doc.data.value) {
      emit(doc.data.key, doc.data.value);
    }
  },
};

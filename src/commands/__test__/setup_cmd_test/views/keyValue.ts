import { _emit } from "../../../../views/emit.js";
import { DatumView } from "../../../../views/DatumView.js";
import { DatumDocument } from "../../../../documentControl/DatumDocument.js";

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

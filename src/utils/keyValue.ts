import { DatumDocument } from "../documentControl/DatumDocument";
import { DatumView } from "../views/DatumView";
import { _emit } from "../views/emit";

function emit(key: unknown, value: unknown): void {
  _emit(key, value);
}

export const keyValueView: DatumView<DatumDocument> = {
  name: "key_value_view",
  map: (doc) => {
    if (doc.data.key !== undefined && doc.data.value !== undefined) {
      emit(doc.data.key, doc.data.value);
    }
  },
};

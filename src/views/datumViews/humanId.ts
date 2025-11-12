import { DatumDocument } from "../../documentControl/DatumDocument.js";
import { DatumView } from "../DatumView.js";
import { _emit } from "../emit.js";

type DocType = DatumDocument;
type MapKey = string;
type MapValue = null;
type ReduceValues = null;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const humanIdView: DatumView<DocType, MapKey, MapValue, ReduceValues> = {
  name: "datum_human_id",
  map: (doc) => {
    if (doc.meta && doc.meta.humanId) {
      emit(doc.meta.humanId as string, null);
    }
  },
};

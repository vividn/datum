import { DatumDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";

type DocType = DatumDocument;
type MapKey = string;
type MapValue = null;
type ReduceValues = undefined;

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

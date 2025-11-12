import { DatumData, EitherDocument } from "../../documentControl/DatumDocument.js";
import { DatumView } from "../DatumView.js";
import { _emit } from "../emit.js";

type DocType = EitherDocument;
type MapKey = string;
type MapValue = null;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const fieldView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "datum_fields",
  map: (doc) => {
    let data: DatumData;
    if (doc.data && doc.meta) {
      data = doc.data as DatumData;
    } else {
      data = doc;
    }
    if (data.field) {
      emit(data.field, null);
    }
  },
  reduce: "_count",
};

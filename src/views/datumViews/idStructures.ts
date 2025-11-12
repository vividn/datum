import {
  DatumDocument,
  EitherDocument,
} from "../../documentControl/DatumDocument.js";
import { DatumView } from "../DatumView.js";
import { _emit } from "../emit.js";

type DocType = EitherDocument;
type MapKey = string | null;
type MapValue = null;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const idStructuresView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValue
> = {
  name: "id_structures",
  map: (doc) => {
    const { meta } = doc as DatumDocument;
    if (meta && meta.idStructure) {
      emit(meta.idStructure, null);
    } else {
      emit(null, null);
    }
  },
  reduce: "_count",
};

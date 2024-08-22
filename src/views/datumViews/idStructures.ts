import {
  DatumDocument,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";

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
  emit,
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

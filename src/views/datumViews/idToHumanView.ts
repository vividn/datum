import { DatumView } from "../DatumView";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { _emit } from "../emit";

type DocType = DatumDocument;
type MapKey = string;
type MapValue = string;
type ReduceValues = null;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const idToHumanView: DatumView<DocType, MapKey, MapValue, ReduceValues> =
  {
    name: "datum_id_to_human_id",
    map: (doc) => {
      if (doc.meta?.humanId) {
        emit(doc._id, doc.meta.humanId);
      }
    },
  };

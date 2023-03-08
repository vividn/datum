import { DatumView } from "../DatumView";
import { DatumMetadata, EitherDocument } from "../../documentControl/DatumDocument";
import { _emit } from "../emit";

type DocType = EitherDocument;
type MapKey = string;
type MapValue = string;
type ReduceValues = undefined;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const idToHumanView: DatumView<DocType, MapKey, MapValue, ReduceValues> =
  {
    name: "datum_id_to_human_id",
    emit,
    map: (doc) => {
      let meta: DatumMetadata;
      if (!doc.meta) {
        return;
      } else {
        meta = doc.meta;
      }
      if (meta.humanId) {
        emit(doc._id, meta.humanId);
      }
    },
  };

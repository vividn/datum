import { DatumView } from "../DatumView";
import {
  DatumDocument,
  DatumMetadata,
} from "../../documentControl/DatumDocument";
import { _emit } from "../emit";

type DocType = DatumDocument;
type MapKey = string;
type MapValue = null;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const subHumanIdView: DatumView<DocType, MapKey, MapValue, ReduceValue> =
  {
    name: "datum_sub_human_id",
    map: (doc) => {
      if (doc.meta?.humanId) {
        const hid = (doc.meta as DatumMetadata).humanId as string;
        let i = hid.length;
        while (i--) {
          emit(hid.slice(0, i + 1), null);
        }
      }
    },
    reduce: "_count",
  };

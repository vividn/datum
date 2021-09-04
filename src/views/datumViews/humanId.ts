import { EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../viewDocument";
import _emit from "../emit";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const humanIdView: DatumView<EitherDocument> = {
  name: "datum_human_id",
  map: (doc) => {
    if (doc.meta && doc.meta.humanId) {
      emit(doc.meta.humanId, null);
    }
  },
};

export const subHumanIdView: DatumView<EitherDocument> = {
  name: "datum_sub_human_id",
  map: (doc) => {
    if (doc.meta && doc.meta.humanId) {
      const hid = doc.meta.humanId;
      let i = hid.length;
      while (i--) {
        emit(hid.slice(0, i + 1), null);
      }
    }
  },
  reduce: "_count",
};


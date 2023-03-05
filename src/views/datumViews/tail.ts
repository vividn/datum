import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import {
  DataOnlyDocument,
  EitherDocument,
} from "../../documentControl/DatumDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const occurTimeView: DatumView<EitherDocument> = {
  name: "datum_occur_time",
  map: (doc) => {
    if (doc.data && doc.data.occurTime) {
      emit(doc.data.occurTime, {
        field: doc.data.field,
        dur: doc.data.dur,
        stop: doc.data.stop,
      });
    } else if ((doc as DataOnlyDocument).occurTime) {
      doc = doc as DataOnlyDocument;
      emit(doc.occurTime, { field: doc.field, dur: doc.dur, stop: doc.stop });
    }
  },
};

export const createTimeView: DatumView<EitherDocument> = {
  name: "datum_create_time",
  map: (doc) => {
    if (doc.meta && doc.meta.createTime) {
      emit(doc.meta.createTime, null);
    }
  },
};

export const modifyTimeView: DatumView<EitherDocument> = {
  name: "datum_modify_time",
  map: (doc) => {
    if (doc.meta && doc.meta.modifyTime) {
      emit(doc.meta.modifyTime, null);
    }
  },
};

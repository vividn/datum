import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import {
  DatumData,
  DatumMetadata,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { isoDateOrTime } from "../../time/timeUtils";

type MapKey = isoDateOrTime;
type MapValue = {
  field?: string;
  dur?: string;
};

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const occurTimeView: DatumView<EitherDocument, MapKey, MapValue, undefined> = {
  name: "datum_occur_time",
  emit,
  map: (doc) => {
    let data: DatumData;
    if (doc.data && doc.meta) {
      data = doc.data;
    } else {
      data = doc;
    }
    if (data.occurTime) {
      emit(data.occurTime, {
        field: data.field,
        dur: data.dur,
      });
    }
  },
};

export const createTimeView: DatumView<EitherDocument, MapKey, MapValue, undefined> = {
  name: "datum_create_time",
  emit,
  map: (doc) => {
    let data: DatumData;
    let meta: DatumMetadata;
    if (doc.data && doc.meta) {
      meta = doc.meta;
      data = doc.data;
    } else {
      return;
    }
    if (meta.createTime) {
      emit(meta.createTime, {
        field: data.field,
        dur: data.dur,
      });
    }
  },
};

export const modifyTimeView: DatumView<EitherDocument, MapKey, MapValue, undefined> = {
  name: "datum_modify_time",
  emit,
  map: (doc) => {
    let data: DatumData;
    let meta: DatumMetadata;
    if (doc.data && doc.meta) {
      meta = doc.meta;
      data = doc.data;
    } else {
      return;
    }
    if (meta.modifyTime) {
      emit(doc.meta.modifyTime, {
        field: data.field,
        dur: data.dur,
      });
    }
  },
};

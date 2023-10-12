import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import {
  DatumData,
  DatumMetadata,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { isoDateOrTime } from "../../time/timeUtils";

type TimeType = "hybrid" | "occur" | "modify" | "create";
type Field = string | null;
type MapKey = [TimeType, Field, isoDateOrTime];
type MapValue = null;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const timingView: DatumView<
  EitherDocument,
  MapKey,
  MapValue,
  undefined
> = {
  name: "datum_timing",
  emit,
  map: (doc) => {
    let data: DatumData;
    let meta: DatumMetadata | undefined;
    if (doc.data && doc.meta) {
      data = doc.data;
      meta = doc.meta;
    } else {
      data = doc;
    }
    const field = data.field;

    const occurTime = data.occurTime;
    const modifyTime = meta && meta.modifyTime;
    const createTime = meta && meta.createTime;
    const hybridTime = occurTime || modifyTime;

    if (hybridTime) {
      emit(["hybrid", null, hybridTime], null);
      if (field !== undefined) {
        emit(["hybrid", field, hybridTime], null);
      }
    }
    if (occurTime) {
      emit(["occur", null, occurTime], null);
      if (field !== undefined) {
        emit(["occur", field, occurTime], null);
      }
    }
    if (modifyTime) {
      emit(["modify", null, modifyTime], null);
      if (field !== undefined) {
        emit(["modify", field, modifyTime], null);
      }
    }
    if (createTime) {
      emit(["create", null, createTime], null);
      if (field !== undefined) {
        emit(["create", field, createTime], null);
      }
    }
  },
};

import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import {
  DatumData,
  DatumMetadata,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { isoDateOrTime } from "../../time/timeUtils";

export const TIME_METRICS = ["hybrid", "occur", "create", "modify"] as const;
type TimeMetric = (typeof TIME_METRICS)[number];
type Field = string | null;
type UtcOffset = number | null;
type MapKey = [TimeMetric, Field, isoDateOrTime, UtcOffset];
type MapValue = null;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export type TimingViewType = {
  DocType: EitherDocument;
  MapKey: MapKey;
  MapValue: MapValue;
};

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
    const occurUtcOffset = data.occurUtcOffset || null;
    const modifyTime = meta && meta.modifyTime;
    const createTime = meta && meta.createTime;
    const hybridTime = occurTime || createTime;
    const hybridUtcOffset =
      occurTime && occurUtcOffset !== null ? occurUtcOffset : null;

    if (hybridTime) {
      emit(["hybrid", null, hybridTime, hybridUtcOffset], null);
      if (field !== undefined) {
        emit(["hybrid", field, hybridTime, hybridUtcOffset], null);
      }
    }
    if (occurTime) {
      emit(["occur", null, occurTime, occurUtcOffset], null);
      if (field !== undefined) {
        emit(["occur", field, occurTime, occurUtcOffset], null);
      }
    }
    if (modifyTime) {
      emit(["modify", null, modifyTime, null], null);
      if (field !== undefined) {
        emit(["modify", field, modifyTime, null], null);
      }
    }
    if (createTime) {
      emit(["create", null, createTime, null], null);
      if (field !== undefined) {
        emit(["create", field, createTime, null], null);
      }
    }
  },
};

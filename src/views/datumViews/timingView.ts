import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import {
  DatumData,
  DatumMetadata,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { isoDate, isoDatetime } from "../../time/timeUtils";

export const TIME_METRICS = ["hybrid", "occur", "create", "modify"] as const;
type TimeMetric = (typeof TIME_METRICS)[number];
type Field = string | null;
type MapKey = [TimeMetric, Field, isoDatetime];

type LocalDate = isoDate;
// MapValue is used for getting all the entries for a given day (from a local perspective)
// utc datetime is used in the second value or null if the occurTime is just a date, for sorting date values to the top of the list in tailCmd.
type MapValue = [LocalDate, isoDatetime | null];

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

    function getLocalDate(
      time: string,
      offset?: number | undefined
    ): [LocalDate, isoDatetime | null] {
      if (!time.includes("T")) {
        return [time, null];
      }
      if (offset === undefined) {
        return [time.split("T")[0], time];
      }
      const msOffset = offset * 60 * 60 * 1000;
      const dateTime = new Date(time);
      const localDate = new Date(dateTime.getTime() + msOffset)
        .toISOString()
        .split("T")[0];
      return [localDate, time];
    }

    const occurTime = data.occurTime;
    const occurUtcOffset = data.occurUtcOffset;
    const modifyTime = meta && meta.modifyTime;
    const createTime = meta && meta.createTime;
    const hybridTime = occurTime || createTime;
    const hybridUtcOffset =
      occurTime && occurUtcOffset !== undefined ? occurUtcOffset : undefined;
    if (hybridTime) {
      const localDate = getLocalDate(hybridTime, hybridUtcOffset);
      emit(["hybrid", null, hybridTime], localDate);
      if (field !== undefined) {
        emit(["hybrid", field, hybridTime], localDate);
      }
    }
    if (occurTime) {
      const localDate = getLocalDate(occurTime, occurUtcOffset);
      emit(["occur", null, occurTime], localDate);
      if (field !== undefined) {
        emit(["occur", field, occurTime], localDate);
      }
    }
    if (modifyTime) {
      const localDate = getLocalDate(modifyTime);
      emit(["modify", null, modifyTime], localDate);
      if (field !== undefined) {
        emit(["modify", field, modifyTime], localDate);
      }
    }
    if (createTime) {
      const localDate = getLocalDate(createTime);
      emit(["create", null, createTime], localDate);
      if (field !== undefined) {
        emit(["create", field, createTime], localDate);
      }
    }
  },
};

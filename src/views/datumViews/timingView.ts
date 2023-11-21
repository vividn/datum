import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import {
  DatumData,
  DatumMetadata, DatumTime,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { isoDate, isoDatetime } from "../../time/timeUtils";

export const TIME_METRICS = ["hybrid", "occur", "create", "modify"] as const;
type TimeMetric = (typeof TIME_METRICS)[number];
type Field = string | null;
type MapKey = [TimeMetric, Field, isoDatetime];

type LocalDate = isoDate;
// MapValue is used for getting all the entries for a given day (from a local perspective)
// the second value is epoch milliseconds or null if the occurTime is just a date, for sorting date values to the top of the list in tailCmd.
type MapValue = [LocalDate, number | null];

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
      timeStr: string,
      offset?: number | undefined
    ): [LocalDate, number | null] {
      if (!timeStr.includes("T")) {
        return [timeStr, null];
      }
      const dateTime = new Date(timeStr).getTime();

      if (offset === undefined) {
        return [timeStr.split("T")[0], dateTime];
      }
      const msOffset = offset * 60 * 60 * 1000;
      const localDate = new Date(dateTime + msOffset)
        .toISOString()
        .split("T")[0];
      return [localDate, dateTime];
    }

    function dtTransform(time: string | DatumTime | undefined): DatumTime | undefined {
      // TODO: Remove this once all documents are migrated to new format
      if (typeof time === "string") {
        return { utc: time };
      }
      return time;
    }

    const occurTime = dtTransform(data.occurTime);
    const modifyTime = dtTransform(meta && meta.modifyTime);
    const createTime = dtTransform(meta && meta.createTime);
    const hybridTime = dtTransform(occurTime || createTime);

    if (hybridTime) {
      const localDate = getLocalDate(hybridTime.utc, hybridTime.o);
      emit(["hybrid", null, hybridTime.utc], localDate);
      if (field !== undefined) {
        emit(["hybrid", field, hybridTime.utc], localDate);
      }
    }
    if (occurTime) {
      const localDate = getLocalDate(occurTime.utc, occurTime.o);
      emit(["occur", null, occurTime.utc], localDate);
      if (field !== undefined) {
        emit(["occur", field, occurTime.utc], localDate);
      }
    }
    if (modifyTime) {
      const localDate = getLocalDate(modifyTime.utc, modifyTime.o);
      emit(["modify", null, modifyTime.utc], localDate);
      if (field !== undefined) {
        emit(["modify", field, modifyTime.utc], localDate);
      }
    }
    if (createTime) {
      const localDate = getLocalDate(createTime.utc, createTime.o);
      emit(["create", null, createTime.utc], localDate);
      if (field !== undefined) {
        emit(["create", field, createTime.utc], localDate);
      }
    }
  },
};

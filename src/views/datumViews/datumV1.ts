import { _emit } from "../emit";
import { DatumView } from "../DatumView";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { ViewRow } from "../../utils/utilityTypes";
import { isoDateOrTime } from "../../time/timeUtils";

type DocType = DatumDocument;
type MapKey = [string, isoDateOrTime];
type MapValue = string[];

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export type V1MapRow = ViewRow<MapKey, MapValue>;
export type V1ReduceRowGroup1 = ViewRow<[string], number>;

export const datumV1View: DatumView<DocType, MapKey, MapValue, number> = {
  name: "datum_v1_view",
  emit: emit,
  map: (doc) => {
    const data = doc.data;
    if (!data.occurTime || !data.occurUtcOffset || !data.field) {
      return;
    }

    const key: MapKey = [data.field, data.occurTime];

    const offset = data.occurUtcOffset;
    const msOffset = offset * 60 * 60 * 1000;
    const dateTime = new Date(data.occurTime);
    const offsetDateTime = new Date(dateTime.getTime() + msOffset);

    const outputArray = offsetDateTime.toISOString().split(/[TZ]/, 2);

    const offsetPolarity = offset >= 0 ? "+" : "-";
    const offsetHour =
      Math.abs(offset) >= 10
        ? Math.abs(offset) + "00"
        : "0" + Math.abs(offset) + "00";
    const offsetStr = offsetPolarity + offsetHour;
    outputArray.push(offsetStr);

    const iso8601DurationRegex =
      /(-)?P(?:([.,\d]+)Y)?(?:([.,\d]+)M)?(?:([.,\d]+)W)?(?:([.,\d]+)D)?(?:T(?:([.,\d]+)H)?(?:([.,\d]+)M)?(?:([.,\d]+)S)?)?/;
    const matches = (data.dur || data.duration || "").match(
      iso8601DurationRegex
    );
    const durObj = matches
      ? {
          sign: matches[1] === undefined ? 1 : -1,
          years: matches[2] === undefined ? 0 : Number(matches[2]),
          months: matches[3] === undefined ? 0 : Number(matches[3]),
          weeks: matches[4] === undefined ? 0 : Number(matches[4]),
          days: matches[5] === undefined ? 0 : Number(matches[5]),
          hours: matches[6] === undefined ? 0 : Number(matches[6]),
          minutes: matches[7] === undefined ? 0 : Number(matches[7]),
          seconds: matches[8] === undefined ? 0 : Number(matches[8]),
        }
      : undefined;
    const minutes = durObj
      ? durObj.sign *
        (525600 * durObj.years +
          43800 * durObj.months +
          10080 * durObj.weeks +
          1440 * durObj.days +
          60 * durObj.hours +
          durObj.minutes +
          durObj.seconds / 60)
      : "";
    const state = data.state;

    if (state === false) {
      if (minutes === "") {
        outputArray.push("end");
      } else {
        outputArray.push(String(-1 * minutes));
      }
    } else if (state === true) {
      if (minutes === "") {
        outputArray.push("start");
      } else {
        outputArray.push(String(minutes));
      }
    } else if (state === undefined) {
      outputArray.push(String(minutes));
    } else {
      outputArray.push(String(minutes));
      outputArray.push(String(state));
    }

    emit(key, outputArray);
  },
  reduce: (keysAndDocIds, values, rereduce): number => {
    if (!rereduce) {
      return values.reduce((acc, val) => Math.max(acc, val.length), 0);
    } else {
      return Math.max(...values);
    }
  },
};

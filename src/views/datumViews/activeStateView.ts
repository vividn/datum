import { DatumData, EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import { isoDateOrTime } from "../../time/timeUtils";
import { GenericObject } from "../../GenericObject";

export type StateObject = GenericObject & { id: string | boolean | number };
export type DatumState = StateObject | StateObject[] | null;

type DocType = EitherDocument;
type MapKey = [string, isoDateOrTime];
type MapValue = DatumState;
type ReduceValues = undefined;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const activeStateView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValues
> = {
  name: "active_state",
  emit,
  map: (doc) => {
    let data: DatumData;
    if (doc.data && doc.meta) {
      data = doc.data;
    } else {
      data = doc;
    }
    const occurTime = data.occurTime;
    const field = data.field;
    if (!occurTime || !field) {
      return;
    }
    const duration =
      data.dur !== undefined
        ? data.dur
        : data.state === undefined
          ? null
          : undefined;
    const state = data.state !== undefined ? data.state : true;
    const lastState =
      data.lastState !== undefined ? data.lastState : state === false;

    function parseISODuration(duration: string) {
      const regex =
        /(-)?P(?:(\d+)Y)?(?:(\d+)M)?(?:(\d+)D)?T?(?:(\d+)H)?(?:(\d+)M)?(?:(\d+(?:\.\d+)?)S)?/;
      const matches = duration.match(regex);

      if (!matches) {
        throw new Error("Invalid ISO 8601 duration format");
      }

      const negativeCoefficient = matches[1] ? -1 : 1;
      const years = parseInt(matches[2], 10) || 0;
      const months = parseInt(matches[3], 10) || 0;
      const days = parseInt(matches[4], 10) || 0;
      const hours = parseInt(matches[5], 10) || 0;
      const minutes = parseInt(matches[6], 10) || 0;
      const seconds = parseFloat(matches[7]) || 0;

      // Assuming 365.25 days in a year and 30 days in a month
      const totalSeconds =
        negativeCoefficient *
        (years * 365.25 * 24 * 60 * 60 +
          months * 30 * 24 * 60 * 60 +
          days * 24 * 60 * 60 +
          hours * 60 * 60 +
          minutes * 60 +
          seconds);

      return totalSeconds;
    }
    function subtractSecondsFromTime(
      time: isoDateOrTime,
      seconds: number,
    ): isoDateOrTime {
      const newTime = new Date(time);
      newTime.setSeconds(newTime.getSeconds() - seconds);
      return newTime.toISOString();
    }

    if (duration === null) {
      if (lastState === null && state !== null) {
        emit([field, occurTime.utc], false);
      }
      return;
    }
    if (duration === undefined) {
      emit([field, occurTime.utc], state);
      return;
    }

    const seconds = parseISODuration(duration);
    if (seconds > 0) {
      const blockBegin = subtractSecondsFromTime(occurTime.utc, seconds);
      const stateAfterBlock = lastState === null ? false : lastState;
      emit([field, blockBegin], state);
      emit([field, occurTime.utc], stateAfterBlock);
    } else if (seconds < 0) {
      const holeBegin = subtractSecondsFromTime(
        occurTime.utc,
        Math.abs(seconds),
      );
      emit([field, holeBegin], false);
      emit([field, occurTime.utc], state);
    } else {
      return;
    }
  },
};

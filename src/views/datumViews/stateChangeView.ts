import { DatumData, EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";
import { isoDateOrTime } from "../../time/timeUtils";
import { DatumState } from "../../state/normalizeState";

type DocType = EitherDocument;
type MapKey = [string, isoDateOrTime];
type LastState = DatumState;
type ActiveState = DatumState;
type MapValue = [LastState, ActiveState];
type ReduceValues = null;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const stateChangeView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValues
> = {
  name: "state_change",
  map: (doc) => {
    let data: DatumData;
    if (doc.data && doc.meta) {
      data = doc.data as DatumData;
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
      data.lastState !== undefined
        ? (data.lastState as DatumState)
        : state === false;

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
        // if an occurrence is recorded when the field is not being tracked, start tracking it
        emit([field, occurTime.utc], [lastState, false]);
      }
      return;
    }
    if (duration === undefined) {
      emit([field, occurTime.utc], [lastState, state]);
      return;
    }

    const seconds = parseISODuration(duration);
    if (seconds > 0) {
      const blockBegin = subtractSecondsFromTime(occurTime.utc, seconds);
      // if a block occurs when the field is not being tracked, start tracking it
      const stateAfterBlock = lastState === null ? false : lastState;
      emit([field, blockBegin], [lastState, state]);
      emit([field, occurTime.utc], [state, stateAfterBlock]);
    } else if (seconds < 0) {
      const holeBegin = subtractSecondsFromTime(
        occurTime.utc,
        Math.abs(seconds),
      );
      emit([field, holeBegin], [state, false]);
      emit([field, occurTime.utc], [false, state]);
    } else {
      return;
    }
  },
  reduce: "_count",
};

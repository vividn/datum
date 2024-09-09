import { DatumData, EitherDocument } from "../../documentControl/DatumDocument";
import { isoDateOrTime } from "../../time/timeUtils";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";

type DocType = EitherDocument;
type MapKey = [string, isoDateOrTime];
type MapValue = number; // 1 for in block, -1 for not in a block, 0 for state changes that aren't block based
type ReduceValues = null;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const overlappingBlockView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValues
> = {
  name: "duration_blocks",
  map: (doc) => {
    let data: DatumData;
    if (doc.data && doc.meta) {
      data = doc.data as DatumData;
    } else {
      data = doc;
    }
    const occurTime = data.occurTime;
    const field = data.field;
    const dur = data.dur;
    if (!occurTime || !field) {
      return;
    }

    if (!dur) {
      emit([field, occurTime.utc], 0);
      return;
    }

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

    const seconds = parseISODuration(dur);
    if (seconds > 0) {
      const blockBegin = subtractSecondsFromTime(occurTime.utc, seconds);
      const blockEnd = occurTime.utc;
      emit([field, blockBegin], 1);
      emit([field, blockEnd], -1);
    } else if (seconds < 0) {
      const blockBegin = subtractSecondsFromTime(
        occurTime.utc,
        Math.abs(seconds),
      );
      const blockEnd = occurTime.utc;
      emit([field, blockBegin], 1);
      emit([field, blockEnd], -1);
    } else {
      emit([field, occurTime.utc], 0);
    }
  },
};

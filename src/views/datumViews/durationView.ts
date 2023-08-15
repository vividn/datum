import { DatumData, EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";

type DocType = EitherDocument;
type MapKey = string;
type MapValue = null;
type ReduceValues = undefined;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const durationView: DatumView<DocType, MapKey, MapValue, ReduceValues> =
  {
    name: "datum_duration",
    emit,
    map: (doc) => {
      let data: DatumData;
      if (doc.data && doc.meta) {
        data = doc.data;
      } else {
        data = doc;
      }
      const occurTime = data.occurTime;
      if (!occurTime) {
        return;
      }

      const duration = data.duration || data.dur;
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
        // You may adjust this as needed for your application
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
      function addSecondsToTime(time: Date, seconds: number) {
        const newTime = new Date(time);
        newTime.setSeconds(newTime.getSeconds() + seconds);
        return newTime;
      }
    },
  };

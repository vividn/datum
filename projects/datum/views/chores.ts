import { _emit } from "../../../src/views/emit";
import { DatumDocument } from "../../../src/documentControl/DatumDocument";
import { DatumTime, isoDate, isoDateOrTime } from "../../../src/time/timeUtils";
import { DatumView } from "../../../src/views/DatumView";

export const ZERO_DATE = "0000-00-00" as const;
type ChoreDoc = DatumDocument<{
  field: string;
  nextDate?: isoDate;
  nextTime?: DatumTime;
}>;

type DocType = ChoreDoc;
type MapKey = string; // chore name;
type MapValue = {
  time: isoDateOrTime;
  next?: isoDateOrTime;
  lastOccur: isoDateOrTime | typeof ZERO_DATE;
};
type ReduceValue = MapValue;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const choreView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "chores",
  emit,
  map: (doc: ChoreDoc) => {
    const { data, meta } = doc;
    if (!data || !meta) {
      return;
    }
    if (!data.field) {
      return;
    }
    if (!data.occurTime && !data.nextTime && !data.nextDate) {
      return;
    }
    const { nextDate, nextTime, occurTime } = data;
    const time = occurTime ? data.occurTime : meta.createTime;
    if (time === undefined) {
      return;
    }
    let next: isoDateOrTime | null | undefined = undefined;
    if (nextTime) {
      if (nextDate) {
        const nextTimeParsed = new Date(nextTime.utc);
        const nextDateParsed = new Date(nextDate);
        nextTimeParsed.setFullYear(
          nextDateParsed.getFullYear(),
          nextDateParsed.getMonth(),
          nextDateParsed.getDate(),
        );
        next = nextTimeParsed.toISOString();
      } else {
        next = nextTime.utc;
      }
    } else if (nextDate) {
      next = nextDate;
    }
    emit(data.field, {
      time: time.utc,
      next,
      lastOccur: occurTime ? time.utc : ZERO_DATE,
    });
  },
  reduce: (_keysIds, values, _rereduce) => {
    return values.reduce((reduced, currentValue) => {
      const isLatest = currentValue.time > reduced.time;
      const latestNext = isLatest ? currentValue.next : reduced.next;
      const latestTime = isLatest ? currentValue.time : reduced.time;
      const latestOccur =
        currentValue.lastOccur > reduced.lastOccur
          ? currentValue.lastOccur
          : reduced.lastOccur;
      return {
        time: latestTime,
        next: latestNext,
        lastOccur: latestOccur,
      };
    });
  },
};

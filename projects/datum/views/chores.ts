import { _emit } from "../../../src/views/emit";
import { DatumDocument } from "../../../src/documentControl/DatumDocument";
import { DatumTime, isoDate, isoDateOrTime } from "../../../src/time/timeUtils";
import { DatumView } from "../../../src/views/DatumView";

export type ChoreDoc = DatumDocument<{
  field: string;
  nextDate?: isoDate;
  nextTime?: DatumTime;
}>;

type DocType = ChoreDoc;
type MapKey = string; // chore name;
type MapValue = {
  time: isoDateOrTime;
  next?: isoDateOrTime;
  iti?: number;
  lastOccur: isoDateOrTime;
};
type ReduceValue = MapValue;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const choreView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "chores",
  emit,
  map: (doc: ChoreDoc) => {
    const ZERO_DATE = "0000-00-00" as const;
    const { data, meta } = doc;
    if (!data || !meta) {
      return;
    }
    if (!data.field) {
      return;
    }
    if (!data.nextTime && !data.nextDate) {
      return;
    }
    const { nextDate, nextTime, occurTime } = data;
    const time = occurTime ? data.occurTime : meta.createTime;
    if (time === undefined) {
      return;
    }
    let next: isoDateOrTime | null | undefined = undefined;
    let fullDay = false;
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
      fullDay = true;
    }
    // ITI (inter time interval) is used to sort chores by frequency of occurring
    // Round up to a full day and then add the percentage of the day when the occurrence happened so that chores that
    // are done later in the day are sorted after chores that are done earlier
    let iti: number | undefined = undefined;
    if (next && occurTime) {
      if (fullDay) {
        const nextWithOffset = occurTime.o
          ? new Date(new Date(next).getTime() - occurTime.o * 3600000)
          : new Date(next);
        iti = Math.ceil(
          (nextWithOffset.getTime() - new Date(time.utc).getTime()) /
          (1000 * 60 * 60 * 24),
        );
        const offsetOccur = new Date(occurTime.utc);
        offsetOccur.setHours(offsetOccur.getHours() + (occurTime.o || 0));
        const timeSinceMidnight =
          offsetOccur.getTime() -
          new Date(offsetOccur.toDateString()).getTime();
        iti += timeSinceMidnight / (1000 * 60 * 60 * 24);
      } else {
        iti =
          (new Date(next).getTime() - new Date(time.utc).getTime()) /
          (1000 * 60 * 60 * 24);
      }
    }

    emit(data.field, {
      time: time.utc,
      next,
      lastOccur: occurTime ? time.utc : ZERO_DATE,
      iti,
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
      // TODO: Kind of hacky, may not grab the latest ITI if the most recent instance is a shirk
      const iti = reduced.iti === undefined ? currentValue.iti : isLatest && currentValue.iti !== undefined ? currentValue.iti : reduced.iti;
      return {
        time: latestTime,
        next: latestNext,
        lastOccur: latestOccur,
        iti: iti,
      };
    });
  },
};

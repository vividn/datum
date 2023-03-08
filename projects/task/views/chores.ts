import { _emit } from "../../../src/views/emit";
import { TaskDoc } from "./inbox";
import { DatumDocument } from "../../../src/documentControl/DatumDocument";
import {
  isoDate,
  isoDateOrTime,
  isoDatetime,
} from "../../../src/time/timeUtils";
import { DatumView } from "../../../src/views/DatumView";

type ChoreDoc = TaskDoc &
  DatumDocument<{
    type: "maintain";
    nextDate?: isoDate;
    nextTime?: isoDatetime;
  }>;

type DocType = ChoreDoc;
type MapKey = string; // chore name;
type MapValue = {
  occur: isoDateOrTime;
  next?: isoDateOrTime;
  lastDone: isoDateOrTime | "#not done#";
};
type ReduceValues = {
  default: MapValue;
};

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const choreView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValues
> = {
  name: "chores",
  emit,
  map: (doc: ChoreDoc) => {
    if (doc.data && doc.data.type === "maintain" && doc.data.occurTime) {
      const { nextDate, nextTime, occurTime, done } = doc.data;
      let next: isoDateOrTime | null | undefined = undefined;
      if (nextTime) {
        if (nextDate) {
          const nextTimeParsed = new Date(nextTime);
          const nextDateParsed = new Date(nextDate);
          nextTimeParsed.setFullYear(
            nextDateParsed.getFullYear(),
            nextDateParsed.getMonth(),
            nextDateParsed.getDate()
          );
          next = nextTimeParsed.toISOString();
        } else {
          next = nextTime;
        }
      } else if (nextDate) {
        next = nextDate;
      }
      emit(doc.data.task, {
        occur: occurTime,
        next,
        lastDone: done ? occurTime : "#not done#",
      });
    }
  },
  reduce: {
    default: (keysIds, values, _rereduce) => {
      return values.reduce((reduced, currentValue) => {
        const isLatest = currentValue.occur > reduced.occur;
        const latestNext = isLatest ? currentValue.next : reduced.next;
        const latestOccur = isLatest ? currentValue.occur : reduced.occur;
        const latestDoneOccur =
          currentValue.lastDone > reduced.lastDone
            ? currentValue.lastDone
            : reduced.lastDone;
        return {
          occur: latestOccur,
          next: latestNext,
          lastDone: latestDoneOccur,
        };
      });
    },
  },
};

import { _emit } from "../../src/views/emit";
import { TaskDoc } from "./inbox";
import { DatumDocument } from "../../src/documentControl/DatumDocument";
import { isoDate, isoDateOrTime, isoDatetime } from "../../src/time/timeUtils";
import { DatumView } from "../../src/views/viewDocument";

function emit(doc: ChoreMapRow["key"], value: ChoreMapRow["value"]) {
  _emit(doc, value);
}

type ChoreDoc = TaskDoc &
  DatumDocument<{
    type: "maintain";
    nextDate?: isoDate;
    nextTime?: isoDatetime;
  }>;

type ChoreMapRow = {
  key: string; // chore name
  value: {
    occur: isoDateOrTime;
    next?: isoDateOrTime;
    lastDone: isoDateOrTime | "";
  };
};

export const choreView: DatumView<ChoreDoc> = {
  name: "chores",
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
  reduce: (keysIds, values: ChoreMapRow["value"][], _rereduce) => {
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
        lastDone: latestDoneOccur,
        next: latestNext,
      };
    });
  },
};

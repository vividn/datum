import { _emit } from "../../src/views/emit";
import { TaskDoc } from "./inbox";
import { DatumDocument } from "../../src/documentControl/DatumDocument";
import { isoDate, isoDateOrTime, isoDatetime } from "../../src/time/timeUtils";
import { DatumView } from "../../src/views/viewDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

type ChoreDoc = TaskDoc &
  DatumDocument<{
    type: "maintain";
    nextDate?: isoDate;
    nextTime?: isoDatetime;
  }>;

type ChoreMapRow = {
  key: string // chore name
  value: { occur: isoDateOrTime, next?: isoDateOrTime }
}

export const choreView: DatumView<ChoreDoc> = {
  name: "chores",
  map: (doc: ChoreDoc) => {
    if (doc.data && doc.data.type === "maintain" && doc.data.occurTime) {
      let next: isoDateOrTime | null | undefined = undefined;
      if (doc.data.nextTime) {
        if (doc.data.nextDate) {
          const nextTime = new Date(doc.data.nextTime);
          const nextDate = new Date(doc.data.nextDate);
          nextTime.setFullYear(
            nextDate.getFullYear(),
            nextDate.getMonth(),
            nextDate.getDate()
          );
          next = nextTime.toISOString();
        } else {
          next = doc.data.nextTime;
        }
      } else if (doc.data.nextDate) {
        next = doc.data.nextDate;
      }
      emit(doc.data.task, { occur: doc.data.occurTime, next });
    }
  },
  reduce: (keysIds, values: ChoreMapRow["value"][], rereduce) => {
    return values.reduce((reduced, currentValue) => {
      const isLatest = currentValue.occur > reduced.occur;
      if (isLatest) {
            return currentValue;
        } else {
            return reduced;
        }
    });
  }
};

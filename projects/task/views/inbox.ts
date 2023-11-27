import { DatumView } from "../../../src/views/DatumView";
import { DatumDocument } from "../../../src/documentControl/DatumDocument";
import { _emit } from "../../../src/views/emit";
import { DatumTime, isoDateOrTime } from "../../../src/time/timeUtils";

export type TaskDoc = DatumDocument<{
  task: string;
  done?: boolean;
  type?: string;
  proj?: string;
}>;

type DocType = TaskDoc;
type MapKey = isoDateOrTime | "unknown";
type MapValue = string;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const inboxView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "inbox",
  emit,
  map: (doc: TaskDoc) => {
    if (!doc.data) {
      return;
    }
    const data = doc.data;
    const meta = doc.meta;
    function dtTransform(time: string | DatumTime | undefined): DatumTime | undefined {
      // TODO: Remove this once all documents are migrated to new format
      if (typeof time === "string") {
        return { utc: time };
      }
      return time;
    }
    const createTime = (dtTransform(meta.createTime) || { utc: "unknown" }).utc;
    const taskName = data.task;
    const humanId = meta.humanId;
    if (data.proj || data.done || (data.type && data.type !== "inbox")) {
      return;
    }
    let quickId;
    if (humanId !== undefined) {
      quickId = humanId.slice(0, 5);
    } else {
      quickId = doc._id.slice(0, 10);
    }
    emit(createTime, `(${quickId}) ${taskName}`);
  },
  reduce: "_count",
};

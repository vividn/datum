import { DatumTime, isoDatetime } from "../../../src/time/timeUtils";
import { DatumView } from "../../../src/views/DatumView";
import { _emit } from "../../../src/views/emit";
import { TaskDoc } from "./inbox";

type DocType = TaskDoc;

type Project = string;
type Status = "pending" | "in_progress" | "done";
type Timestamp = isoDatetime;
type MapKey = [Project, Status, Timestamp];

type TaskName = string;
type MapValue = TaskName;

type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const projectView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "project",
  emit,
  map: (doc: TaskDoc) => {
    if (!doc.data) {
      return;
    }
    const data = doc.data;
    const meta = doc.meta;
    function dtTransform(
      time: string | DatumTime | undefined,
    ): DatumTime | undefined {
      // TODO: Remove this once all documents are migrated to new format
      if (typeof time === "string") {
        return { utc: time };
      }
      return time;
    }

    const createTime = (dtTransform(meta.createTime) || { utc: "unknown" }).utc;

    const proj = data.proj || "inbox";
    const status = data.done
      ? "done"
      : data.comment
        ? "in_progress"
        : "pending";
    emit([proj, status, createTime], data.task);
  },
  reduce: "_count",
};

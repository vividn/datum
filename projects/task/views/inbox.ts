import { DatumView } from "../../../src/views/viewDocument";
import { DatumDocument } from "../../../src/documentControl/DatumDocument";
import { _emit } from "../../../src/views/emit";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export type TaskDoc = DatumDocument<{
  task: string;
  done?: boolean;
  type?: string;
  proj?: string;
}>;

export const inboxView: DatumView<TaskDoc> = {
  name: "inbox",
  map: (doc: TaskDoc) => {
    if (!doc.data) {
      return;
    }
    const data = doc.data;
    const meta = doc.meta;
    const createTime = meta.createTime || "unknown";
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
  reduce: {
    count: "_count",
  },
};

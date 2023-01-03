import { _emit } from "../../src/views/emit";
import { TaskDoc } from "./inbox";
import { DatumView } from "../../src/views/viewDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const projectView: DatumView<TaskDoc> = {
  name: "project",
  map: (doc: TaskDoc) => {
    if (!doc.data) {
      return;
    }
    emit([doc.data.proj, doc.data.done || false], doc.data.task);
  },
  reduce: {
    count: "_count",
  },
};

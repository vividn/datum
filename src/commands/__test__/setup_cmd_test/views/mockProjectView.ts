import { _emit } from "../../../../views/emit";
import { DatumView } from "../../../../views/DatumView";

function emit(key: unknown, value: unknown): void {
  _emit(key, value);
}

export const mockProjectView: DatumView<any, any, any, undefined> = {
  name: "mock_project_view",
  emit,
  map: (doc) => {
    if (doc.data.key && doc.data.value) {
      emit(doc.data.key, doc.data.value);
    }
  },
};

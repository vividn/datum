import { _emit } from "../../../src/views/emit";
import { DatumView } from "../../../src/views/DatumView";

function emit(key: unknown, value: unknown): void {
  _emit(key, value);
}

export const reduceOrderingView: DatumView<
  any,
  any,
  any,
  { keysIds: any; values: any; rereduce: boolean }
> = {
  name: "reduce_ordering_view",
  emit,
  map: (_doc) => {
    emit(Math.random(), Math.random());
    emit(Math.random(), Math.random());
    emit(Math.random(), Math.random());
    emit(Math.random(), Math.random());
  },
  reduce: (keysIds, values, rereduce) => {
    return { keysIds, values, rereduce };
  },
};

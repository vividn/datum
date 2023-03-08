import { DatumView } from "../../../src/views/DatumView";
import { FinanceDoc } from "./balance";
import { isoDateOrTime } from "../../../src/time/timeUtils";
import { _emit } from "../../../src/views/emit";

type DocType = FinanceDoc;
type MapKey = [string, string, string, isoDateOrTime?];
type MapValue = number;
type ReduceValues = {
  default: number;
};

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const categorizedBalanceView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValues
> = {
  name: "categorizedBalance",
  emit,
  map: (doc: FinanceDoc) => {
    const getAccType = (name: string) => {
      switch (true) {
        case /^[A-Z]/.test(name):
          return "Assets";
        case /^[a-z]/.test(name):
          return "expenses";
        case /^\+|^_/.test(name):
          return "+income";
        default:
          return name[0];
      }
    };

    const data = doc.data;
    if (data.type === "tx") {
      const amount = data.reverse === true ? data.amount * -1 : data.amount;
      emit(
        [getAccType(data.acc), data.curr, data.acc, data.occurTime],
        -amount
      );
      emit([getAccType(data.to), data.curr, data.to, data.occurTime], amount);
    }
    if (data.type === "xc") {
      emit(
        [getAccType(data.acc1), data.curr1, data.acc1, data.occurTime],
        -data.amount1
      );
      emit(
        [getAccType(data.acc2), data.curr2, data.acc2, data.occurTime],
        data.amount2
      );
    }
  },
  reduce: {
    default: "_sum",
  },
};

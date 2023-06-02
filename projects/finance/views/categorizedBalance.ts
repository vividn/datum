import { DatumView } from "../../../src/views/DatumView";
import { FinanceDoc } from "./balance";
import { isoDateOrTime } from "../../../src/time/timeUtils";
import { _emit } from "../../../src/views/emit";

type DocType = FinanceDoc;
type MapKey = [string, string, string, isoDateOrTime?];
type MapValue = number;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const categorizedBalanceView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValue
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
    const occurTime = data.effectiveTime || data.effectiveDate || data.occurTime;
    if (data.type === "tx") {
      const amount = data.reverse === true ? data.amount * -1 : data.amount;
      emit(
        [getAccType(data.acc), data.curr, data.acc, occurTime],
        -amount
      );
      emit([getAccType(data.to), data.curr, data.to, occurTime], amount);
    }
    if (data.type === "xc") {
      const occurTime1 = data.effectiveTime1 || data.effectiveDate1 || occurTime;
      const occurTime2 = data.effectiveTime2 || data.effectiveDate2 || occurTime;
      emit(
        [getAccType(data.acc1), data.curr1, data.acc1, occurTime1],
        -data.amount1
      );
      emit(
        [getAccType(data.acc2), data.curr2, data.acc2, occurTime2],
        data.amount2
      );
    }
  },
  reduce: "_sum",
};

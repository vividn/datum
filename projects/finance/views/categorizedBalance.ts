import { DatumView } from "../../../src/views/DatumView";
import { FinanceDoc } from "./balance";
import { isoDateOrTime } from "../../../src/time/timeUtils";
import { _emit } from "../../../src/views/emit";
import { DatumTime } from "../../../src/time/datumTime";

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
    function dtTransform(
      time: string | DatumTime | undefined
    ): DatumTime | undefined {
      // TODO: Remove this once all documents are migrated to new format
      if (typeof time === "string") {
        return { utc: time };
      }
      return time;
    }
    const data = doc.data;
    const occurDatumTime = dtTransform(
      data.effectiveTime || data.effectiveDate || data.occurTime
    );
    if (occurDatumTime === undefined) {
      return;
    }
    const occurTime = occurDatumTime.utc;
    const occurTime1 = dtTransform(
      data.effectiveTime1 || data.effectiveDate1 || occurTime
    )!.utc;
    const occurTime2 = dtTransform(
      data.effectiveTime2 || data.effectiveDate2 || occurTime
    )!.utc;
    if (data.type === "tx") {
      const amount = data.reverse === true ? data.amount * -1 : data.amount;
      emit([getAccType(data.acc), data.curr, data.acc, occurTime1], -amount);
      emit([getAccType(data.to), data.curr, data.to, occurTime2], amount);
    }
    if (data.type === "xc") {
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

import { _emit } from "../../../src/views/emit";
import { DatumDocument } from "../../../src/documentControl/DatumDocument";
import { DatumView } from "../../../src/views/DatumView";
import { isoDateOrTime } from "../../../src/time/timeUtils";

export type TxDoc = DatumDocument<{
  type: "tx";
  curr: string;
  acc: string;
  amount: number;
  to: string;
  reverse?: boolean;
  comment?: string | string[];
  effectiveTime?: isoDateOrTime;
}>;
export type EqDoc = DatumDocument<{
  type: "eq";
  curr: string;
  acc: string;
  bal: number;
  comment?: string | string[];
  effectiveTime?: isoDateOrTime;
}>;
export type XcDoc = DatumDocument<{
  type: "xc";
  acc1: string;
  amount1: number;
  curr1: string;
  acc2: string;
  amount2: number;
  curr2: string;
  comment?: string | string[];
  effectiveTime?: isoDateOrTime;
  effectiveTime1?: isoDateOrTime;
  effectiveTime2?: isoDateOrTime;
}>;

export type FinanceDoc = TxDoc | EqDoc | XcDoc;

type DocType = FinanceDoc;
type MapKey = [string, string, isoDateOrTime | undefined, string];
type MapValue = number;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const balanceView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "balance",
  emit,
  map: (doc: FinanceDoc) => {
    const data = doc.data;
    const occurTime = data.effectiveTime ?? data.occurTime;
    if (data.type === "tx") {
      const amount = data.reverse === true ? data.amount * -1 : data.amount;
      emit([data.acc, data.curr, occurTime, data.to], -amount);
      emit([data.to, data.curr, occurTime, data.acc], amount);
    }
    if (data.type === "xc") {
      const occurTime1 = data.effectiveTime1 ?? occurTime;
      const occurTime2 = data.effectiveTime2 ?? occurTime;
      emit([data.acc1, data.curr1, occurTime1, data.acc2], -data.amount1);
      emit([data.acc2, data.curr2, occurTime2, data.acc1], data.amount2);
    }
  },
  reduce: "_sum",
  options: {
    collation: "raw",
  },
};

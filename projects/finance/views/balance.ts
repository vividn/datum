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
}>;
export type EqDoc = DatumDocument<{
  type: "eq";
  curr: string;
  acc: string;
  bal: number;
  comment?: string | string[];
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
}>;

export type FinanceDoc = TxDoc | EqDoc | XcDoc;

type DocType = FinanceDoc;
type MapKey = [string, string, isoDateOrTime?];
type MapValue = number;
type ReduceValues = {
  default: number;
};

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const balanceView: DatumView<DocType, MapKey, MapValue, ReduceValues> = {
  name: "balance",
  emit,
  map: (doc: FinanceDoc) => {
    const data = doc.data;
    if (data.type === "tx") {
      const amount = data.reverse === true ? data.amount * -1 : data.amount;
      emit([data.acc, data.curr, data.occurTime], -amount);
      emit([data.to, data.curr, data.occurTime], amount);
    }
    if (data.type === "xc") {
      emit([data.acc1, data.curr1, data.occurTime], -data.amount1);
      emit([data.acc2, data.curr2, data.occurTime], data.amount2);
    }
  },
  reduce: {
    default: "_sum",
  },
  options: {
    collation: "raw",
  },
};

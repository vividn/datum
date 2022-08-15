import { _emit } from "../../src/views/emit";
import { DatumDocument } from "../../src/documentControl/DatumDocument";
import { DatumView } from "../../src/views/viewDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

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
  amount: number;
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

export type FinDoc = TxDoc | EqDoc | XcDoc;

export const balanceView: DatumView<FinDoc> = {
  name: "balance",
  map: (doc: FinDoc) => {
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
  reduce: "_sum",
  options: {
    collation: "raw",
  },
};

export const categorizedBalanceView: DatumView<FinDoc> = {
  name: "categorizedBalance",
  map: (doc: FinDoc) => {
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
      emit([getAccType(data.acc), data.curr, data.acc, data.occurTime], -amount);
      emit([getAccType(data.to), data.curr, data.to, data.occurTime], amount);
    }
    if (data.type === "xc") {
      emit([getAccType(data.acc1), data.curr1, data.acc1, data.occurTime], -data.amount1);
      emit([getAccType(data.acc2), data.curr2, data.acc2, data.occurTime], data.amount2);
    }
  },
  reduce: "_sum",
}

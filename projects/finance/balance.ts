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
  comment?: string | string[];
}>
export type EqDoc = DatumDocument<{
  type: "eq";
  curr: string;
  acc: string;
  amount: number;
  comment?: string | string[];
}>
export type XcDoc = DatumDocument<{
  type: "xc";
  acc1: string;
  amount1: number;
  curr1: string;
  acc2: string;
  amount2: number;
  curr2: string;
  comment?: string | string[];
}>

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
  reduce: "_sum"
};
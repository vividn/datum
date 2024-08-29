import { _emit } from "../../../src/views/emit";
import { DatumDocument } from "../../../src/documentControl/DatumDocument";
import { DatumView } from "../../../src/views/DatumView";
import { isoDateOrTime } from "../../../src/time/timeUtils";
import { DatumTime } from "../../../src/time/datumTime";

export type TxDoc = DatumDocument<{
  type: "tx";
  curr: string;
  acc: string;
  amount: number;
  to: string;
  reverse?: boolean;
  comment?: string | string[];
  effectiveTime?: isoDateOrTime;
  effectiveDate?: isoDateOrTime;
}>;
export type EqDoc = DatumDocument<{
  type: "eq";
  curr: string;
  acc: string;
  bal: number;
  comment?: string | string[];
  effectiveTime?: isoDateOrTime;
  effectiveDate?: isoDateOrTime;
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
  effectiveDate?: isoDateOrTime;
  effectiveTime1?: isoDateOrTime;
  effectiveDate1?: isoDateOrTime;
  effectiveTime2?: isoDateOrTime;
  effectiveDate2?: isoDateOrTime;
}>;

export type FinanceDoc = TxDoc | EqDoc | XcDoc;

type DocType = FinanceDoc;
type MapKey = [string, string, isoDateOrTime | null, string];
type MapValue = number;
type ReduceValue = number;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const balanceView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "balance",
  map: (doc: FinanceDoc) => {
    function dtTransform(
      time: string | DatumTime | undefined,
    ): DatumTime | undefined {
      // TODO: Remove this once all documents are migrated to new format
      if (typeof time === "string") {
        return { utc: time };
      }
      return time;
    }
    const data = doc.data;
    const occurDatumTime = dtTransform(
      data.effectiveTime || data.effectiveDate || data.occurTime,
    );
    if (occurDatumTime === undefined) {
      return;
    }
    const occurTime = occurDatumTime.utc;
    const occurTime1 = dtTransform(
      (data.effectiveTime1 || data.effectiveDate1 || occurTime) as
        | DatumTime
        | string
        | undefined,
    )!.utc;
    const occurTime2 = dtTransform(
      (data.effectiveTime2 || data.effectiveDate2 || occurTime) as
        | DatumTime
        | string
        | undefined,
    )!.utc;
    if (data.type === "tx") {
      const amount = data.reverse === true ? data.amount * -1 : data.amount;
      emit([data.acc, data.curr, occurTime1, data.to], -amount);
      emit([data.to, data.curr, occurTime2, data.acc], amount);
    }
    if (data.type === "xc") {
      const occurTime1 =
        data.effectiveTime1 || data.effectiveDate1 || occurTime;
      const occurTime2 =
        data.effectiveTime2 || data.effectiveDate2 || occurTime;
      emit([data.acc1, data.curr1, occurTime1, data.acc2], -data.amount1);
      emit([data.acc2, data.curr2, occurTime2, data.acc1], data.amount2);
    }
  },
  reduce: "_sum",
  options: {
    collation: "raw",
  },
};

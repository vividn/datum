import { _emit } from "../../../src/views/emit";
import { FinanceDoc } from "./balance";
import { DatumView } from "../../../src/views/DatumView";
import { isoDateOrTime } from "../../../src/time/timeUtils";

type DocType = FinanceDoc;
type MapKey = [string, string, isoDateOrTime?];
type MapValue = {
  delta?: number;
  balance?: number;
};
type ReduceValue = {
  delta: number;
  account: string;
  currency: string;
  balance?: number;
  initBalance?: number;
  firstBalanceId?: string;
  error?: {
    id?: string;
    expectedBal?: number;
    calculatedBal?: number;
    offBy: number;
  }[];
} | null;

function emit(key: MapKey, value: MapValue): void {
  _emit(key, value);
}

export const withOrderedReduceView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValue
> = {
  name: "withOrderedReduce",
  emit,
  map: (doc: FinanceDoc) => {
    const data = doc.data;
    const occurTime = data.effectiveTime || data.occurTime;
    if (data.type === "tx") {
      const amount = data.reverse === true ? data.amount * -1 : data.amount;
      emit([data.acc, data.curr, occurTime], { delta: -amount });
      emit([data.to, data.curr, occurTime], { delta: amount });
    }
    if (data.type === "xc") {
      const occurTime1 = data.effectiveTime1 || occurTime;
      const occurTime2 = data.effectiveTime2 || occurTime;
      emit([data.acc1, data.curr1, occurTime1], { delta: -data.amount1 });
      emit([data.acc2, data.curr2, occurTime2], { delta: data.amount2 });
    }
    if (data.type === "eq") {
      emit([data.acc, data.curr, occurTime], { balance: data.bal });
    }
  },
  reduce: (keysAndIds, values, rereduce) => {
    if (!rereduce) {
      return values.reduce(
        (accum: ReduceValue, current, i) => {
          // Must group_level>2 or will get null
          if (accum === null) return null;
          const account = keysAndIds[i][0][0];
          const currency = keysAndIds[i][0][1];
          if (accum.account !== account || accum.currency !== currency)
            return null;

          const id = keysAndIds[i][1];

          // total change since beginning of reduce
          const change = current.delta === undefined ? 0 : current.delta;
          accum.delta += change;

          // update balance if known
          if (accum.balance !== undefined) {
            accum.balance += change;
          }

          // a balance entry should always align with the calculated balance
          if (current.balance !== undefined) {
            if (accum.balance === undefined) {
              accum.balance = current.balance;
              accum.initBalance = current.balance - accum.delta;
              accum.firstBalanceId = id;
            } else if (current.balance !== accum.balance) {
              accum.error = accum.error || [];
              accum.error.push({
                id,
                expectedBal: current.balance,
                calculatedBal: accum.balance,
                offBy: current.balance - accum.balance,
              });

              // update balance anyway to continue on with processing
              accum.balance = current.balance;
            }
          }

          return accum;
        },
        {
          delta: 0,
          account: keysAndIds[0][0][0],
          currency: keysAndIds[0][0][1],
        }
      );
    }
    return values.reduce((accum, current) => {
      if (accum === null || current === null) return null;
      if (
        accum.account !== current.account ||
        accum.currency !== current.currency
      ) {
        return null;
      }

      accum.delta += current.delta;

      if (accum.balance !== undefined) {
        if (current.initBalance !== undefined) {
          // initBalance should always match the balance of the previous chunk
          if (current.initBalance !== accum.balance) {
            accum.error = accum.error || [];
            accum.error.push({
              id: current.firstBalanceId,
              offBy: current.initBalance - accum.balance,
            });
          }
          accum.balance = current.balance;
        } else {
          accum.balance += current.delta;
        }
      } else {
        if (current.balance !== undefined) {
          accum.initBalance = (current.initBalance as number) - accum.delta;
          accum.balance = current.balance;
        }
      }

      return accum;
    });
  },
};

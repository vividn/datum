import { isoDatetime } from "../time/timeUtils";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { durationBlockView } from "../views/datumViews";
import { MyError } from "../errors";
import { HIGH_STRING } from "../utils/startsWith";

type CheckStateType = {
  db: PouchDB.Database;
  field: string;
  startTime?: isoDatetime;
  endTime?: isoDatetime;
};

export async function checkState({
  db,
  field,
  startTime,
  endTime,
}: CheckStateType): Promise<boolean> {
  type StateChangeRow = MapRow<typeof stateChangeView>;
  const startkey = [field, startTime ?? "0000-00-00"];
  const endkey = [field, endTime ?? HIGH_STRING];

  const stateChangeRows = (
    await db.query(stateChangeView.map, {
      reduce: false,
      startkey,
      endkey,
    })
  ).rows as StateChangeRow[];

  if (stateChangeRows.length === 0) {
    return true;
  }

  let previousRow: StateChangeRow = {
    id: "initial_null_state",
    key: [field, "0000-00-00"],
    value: startTime ? [null, stateChangeRows[0].value[0]] : [null, null],
  };
  for (let i = 0; i < stateChangeRows.length; i++) {
    if (previousRow.value[1] === stateChangeRows[i].value[0]) {
      previousRow = stateChangeRows[i];
      continue;
    }
    const context = [previousRow, ...stateChangeRows.slice(i, i + 2)];
    // use the block times view to determine if two blocks are overlapping or a block is overlapping with a state change
    const blockCheckStart = context[0].key[1];
    const blockCheckEnd = context.at(-1)?.key[1] ?? blockCheckStart;
    await checkOverlappingBlocks({
      db,
      field,
      startTime: blockCheckStart,
      endTime: blockCheckEnd,
    });

    // if no blocks are overlapping then this error is recoverable just by changing the lastState value on the offending entry
    throw new LastStateError(context);
  }

  return true;
}

export async function checkOverlappingBlocks({
  db,
  field,
  startTime,
  endTime,
}: CheckStateType): Promise<boolean> {
  type DurationBlockRow = MapRow<typeof durationBlockView>;
  const startkey = [field, startTime ?? "0000-00-00"];
  const endkey = [field, endTime ?? HIGH_STRING];
  const blockTimeRows = (
    await db.query(durationBlockView.map, {
      reduce: false,
      startkey,
      endkey,
    })
  ).rows as DurationBlockRow[];

  // starting value is determined from the first nonzero value, which is assumed to correctly indicate block state
  const firstBlockChange = blockTimeRows.find((row) => row.value !== 0);
  if (blockTimeRows.length === 0 || firstBlockChange === undefined) {
    return true;
  }
  const initialDurationBlockRow: DurationBlockRow = {
    ...firstBlockChange,
    value: firstBlockChange.value * -1,
  };

  blockTimeRows.reduce((lastBlock, curr) => {
    if (lastBlock.value === 1 && curr.value !== -1) {
      throw new OverlappingBlockError(blockTimeRows);
    }
    if (lastBlock.value === -1 && curr.value === -1) {
      throw new OverlappingBlockError(blockTimeRows);
    }
    if (curr.value !== 0) {
      return curr;
    }
    return lastBlock;
  }, initialDurationBlockRow);

  return true;
}

export class LastStateError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, LastStateError.prototype);
  }
}

export class OverlappingBlockError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, OverlappingBlockError.prototype);
  }
}

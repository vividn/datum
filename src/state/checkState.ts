import { isoDatetime } from "../time/timeUtils";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { overlappingBlockView } from "../views/datumViews";
import { MyError } from "../errors";
import { HIGH_STRING } from "../utils/startsWith";
import { pullOutData } from "../utils/pullOutData";
import { updateDoc } from "../documentControl/updateDoc";
import { EitherDocument } from "../documentControl/DatumDocument";
import { OutputArgs } from "../input/outputArgs";
import isEqual from "lodash.isequal";
import { mapReduceOutput } from "../output/mapReduceOutput";

type CheckStateType = {
  db: PouchDB.Database;
  field: string;
  startTime?: isoDatetime;
  endTime?: isoDatetime;
  fix?: boolean;
  outputArgs?: OutputArgs;
};

export async function checkState({
  db,
  field,
  startTime,
  endTime,
  fix,
  outputArgs,
}: CheckStateType): Promise<boolean> {
  type StateChangeRow = MapRow<typeof stateChangeView>;
  const startkey = [field, startTime ?? "0000-00-00"];
  const endkey = [field, endTime ?? HIGH_STRING];

  const stateChangeRows = (
    await db.query(stateChangeView.name, {
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
    if (isEqual(previousRow.value[1], stateChangeRows[i].value[0])) {
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
    if (fix) {
      const oldDoc = (await db.get(context[1].id)) as EitherDocument;
      const { data } = pullOutData(oldDoc);
      console.debug({
        lastState: data.lastState,
        context0: context[0].value[1],
        isEqual: isEqual(data.lastState, context[1].value[0]),
      });
      if (isEqual(data.lastState, context[1].value[0])) {
        await updateDoc({
          db,
          id: context[1].id,
          payload: { lastState: context[0].value[1] },
          outputArgs,
        });
        continue;
      } else {
        throw new LastStateError(
          `Attempted to fix last state error, but last state did not match expected.
data.lastState: ${JSON.stringify(data.lastState)}
context[1].value[0]: ${JSON.stringify(context[1].value[0])}
${mapReduceOutput(context, true)}`,
        );
      }
    }
    throw new LastStateError(
      `Last state error in field ${field} at ${context[1].key[1]}. ids: [${context[0].id}, ${context[1].id}`,
    );
  }

  return true;
}

export async function checkOverlappingBlocks({
  db,
  field,
  startTime,
  endTime,
}: CheckStateType): Promise<boolean> {
  type overlappingBlockRow = MapRow<typeof overlappingBlockView>;
  const startkey = [field, startTime ?? "0000-00-00"];
  const endkey = [field, endTime ?? HIGH_STRING];
  const blockTimeRows = (
    await db.query(overlappingBlockView.name, {
      reduce: false,
      startkey,
      endkey,
    })
  ).rows as overlappingBlockRow[];

  // starting value is determined from the first nonzero value, which is assumed to correctly indicate block state
  const firstBlockChange = blockTimeRows.find((row) => row.value !== 0);
  if (blockTimeRows.length === 0 || firstBlockChange === undefined) {
    return true;
  }
  const initialoverlappingBlockRow: overlappingBlockRow = {
    ...firstBlockChange,
    value: firstBlockChange.value * -1,
  };

  blockTimeRows.reduce((lastBlock, curr) => {
    if (lastBlock.value === 1 && curr.value !== -1) {
      throw new OverlappingBlockError(
        `Overlapping blocks in field ${field} at ${curr.key[1]}. ids: [${lastBlock.id}, ${curr.id} ]}`,
      );
    }
    if (lastBlock.value === -1 && curr.value === -1) {
      throw new OverlappingBlockError(
        `Overlapping blocks in field ${field} at ${curr.key[1]}. ids: [${lastBlock.id}, ${curr.id} ]}`,
      );
    }
    if (curr.value !== 0) {
      return curr;
    }
    return lastBlock;
  }, initialoverlappingBlockRow);

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

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
import { MigrationMapRow } from "../migrations/migrations";

type StateChangeErrorType = {
  message?: string;
  occurTime: isoDatetime;
  ids: string[];
  possibleFixes?: MigrationMapRow[][];
};

export class StateChangeError extends MyError implements StateChangeErrorType {
  occurTime: isoDatetime;
  ids: string[];
  possibleFixes?: MigrationMapRow[][];

  constructor(args: StateChangeErrorType) {
    super(args.message ?? "State change error");
    this.occurTime = args.occurTime;
    this.ids = args.ids;
    this.possibleFixes = args.possibleFixes;

    Object.setPrototypeOf(this, StateChangeError.prototype);
  }
}
export class LastStateError extends StateChangeError {
  constructor(args: StateChangeErrorType) {
    super(args);
    Object.setPrototypeOf(this, LastStateError.prototype);
  }
}

export class OverlappingBlockError extends StateChangeError {
  constructor(args: StateChangeErrorType) {
    super(args);
    Object.setPrototypeOf(this, OverlappingBlockError.prototype);
  }
}

type CheckStateType = {
  db: PouchDB.Database;
  field: string;
  startTime?: isoDatetime;
  endTime?: isoDatetime;
  failOnError?: boolean;
  fix?: boolean;
  outputArgs?: OutputArgs;
};

export type StateErrorSummary = {
  ok: boolean;
  errors: StateChangeError[];
};

type StateChangeRow = MapRow<typeof stateChangeView>;

export async function checkState({
  db,
  field,
  startTime,
  endTime,
  failOnError,
  fix,
  outputArgs,
}: CheckStateType): Promise<StateErrorSummary> {
  failOnError ??= true;
  const summary: StateErrorSummary = {
    ok: true,
    errors: [],
  };
  let startKeyTime = startTime ?? "0000-00-00";
  const endKeyTime = endTime ?? HIGH_STRING;

  refreshFromDbLoop: while (true) {
    const startkey = [field, startKeyTime];
    const endkey = [field, endKeyTime];

    const stateChangeRows = (
      await db.query(stateChangeView.name, {
        reduce: false,
        startkey,
        endkey,
      })
    ).rows as StateChangeRow[];

    if (stateChangeRows.length === 0) {
      break;
    }

    let previousRow: StateChangeRow = {
      id: "initial_null_state",
      key: [field, "0000-00-00"],
      value: startTime ? [null, stateChangeRows[0].value[0]] : [null, null],
    };
    processRowsLoop: for (let i = 0; i < stateChangeRows.length; i++) {
      if (isEqual(previousRow.value[1], stateChangeRows[i].value[0])) {
        previousRow = stateChangeRows[i];
        continue processRowsLoop;
      }
      const context = [previousRow, ...stateChangeRows.slice(i, i + 2)];
      // use the block times view to determine if two blocks are overlapping or a block is overlapping with a state change
      const blockCheckStart = context[0].key[1];
      const blockCheckEnd = context.at(-1)?.key[1] ?? blockCheckStart;
      const overlappingBlocks = await checkOverlappingBlocks({
        db,
        field,
        startTime: blockCheckStart,
        endTime: blockCheckEnd,
        failOnError,
      });
      if (!overlappingBlocks.ok) {
        if (failOnError) {
          throw overlappingBlocks.errors[0];
        }
        summary.ok = false;
        summary.errors.push(...overlappingBlocks.errors);
        // checked context already includes the next row, so skip it
        i++;
        previousRow = stateChangeRows[i];
        continue processRowsLoop;
      }

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
          startKeyTime = context[0].key[1];
          continue refreshFromDbLoop;
        } else {
          const error = new LastStateError({
            message: `Attempted to fix last state error, but last state did not match expected.
data.lastState: ${JSON.stringify(data.lastState)}
context[1].value[0]: ${JSON.stringify(context[1].value[0])}
${mapReduceOutput(context, true)}`,
            ids: [context[0].id, context[1].id],
            occurTime: context[1].key[1],
          });
          if (failOnError) {
            throw error;
          } else {
            summary.ok = false;
            summary.errors.push(error);
          }
        }
      }
      const error = new LastStateError({
        message: `Last state error in field ${field} at ${context[1].key[1]}. ids: [${context[0].id}, ${context[1].id}]`,
        ids: [context[0].id, context[1].id],
        occurTime: context[1].key[1],
      });
      if (failOnError) {
        throw error;
      } else {
        summary.ok = false;
        summary.errors.push(error);
      }
    }
    return summary;
  }
  return summary;
}

export async function checkOverlappingBlocks({
  db,
  field,
  startTime,
  endTime,
  failOnError,
}: CheckStateType): Promise<StateErrorSummary> {
  type OverlappingBlockRow = MapRow<typeof overlappingBlockView>;
  failOnError ??= true;
  const summary: StateErrorSummary = {
    ok: true,
    errors: [],
  };
  const startkey = [field, startTime ?? "0000-00-00"];
  const endkey = [field, endTime ?? HIGH_STRING];
  const blockTimeRows = (
    await db.query(overlappingBlockView.name, {
      reduce: false,
      startkey,
      endkey,
    })
  ).rows as OverlappingBlockRow[];

  // starting value is determined from the first nonzero value, which is assumed to correctly indicate block state
  const firstBlockChange = blockTimeRows.find((row) => row.value !== 0);
  if (blockTimeRows.length === 0 || firstBlockChange === undefined) {
    return summary;
  }
  const initialoverlappingBlockRow: OverlappingBlockRow = {
    ...firstBlockChange,
    value: firstBlockChange.value * -1,
  };

  blockTimeRows.reduce((lastBlock, curr) => {
    if (lastBlock.value === 1 && curr.value !== -1) {
      const messagePrefix =
        curr.value === 1
          ? "A block starts within another"
          : "A state transition occurs within a block";
      const error = new OverlappingBlockError({
        message: `${messagePrefix} in field ${field} at ${curr.key[1]}. ids: [${lastBlock.id}, ${curr.id} ]}`,
        occurTime: curr.key[1],
        ids: [lastBlock.id, curr.id],
      });
      if (failOnError) {
        throw error;
      } else {
        summary.ok = false;
        summary.errors.push(error);
      }
    }

    if (lastBlock.value === -1 && curr.value === -1) {
      const error = new OverlappingBlockError({
        message: `A block ends after another ends indicating overlapping blocks in field ${field} at ${curr.key[1]}. ids: [${lastBlock.id}, ${curr.id} ]}`,
        occurTime: curr.key[1],
        ids: [lastBlock.id, curr.id],
      });
      if (failOnError) {
        throw error;
      } else {
        summary.ok = false;
        summary.errors.push(error);
      }
    }
    if (curr.value !== 0) {
      return curr;
    }
    return lastBlock;
  }, initialoverlappingBlockRow);

  return summary;
}

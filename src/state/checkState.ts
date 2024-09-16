import { isoDatetime } from "../time/timeUtils";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { overlappingBlockView } from "../views/datumViews";
import { MyError } from "../errors";
import { pullOutData } from "../utils/pullOutData";
import { updateDoc } from "../documentControl/updateDoc";
import { EitherDocument } from "../documentControl/DatumDocument";
import { OutputArgs } from "../input/outputArgs";
import isEqual from "lodash.isequal";
import { MigrationMapRow } from "../migrations/migrations";
import { durationBlockView } from "../views/datumViews/durationBlocks";
import { HIGH_STRING } from "../utils/keyEpsilon";
import { extractTimeFromId } from "../utils/extractTimeFromId";

type StateChangeErrorType = {
  message?: string;
  field: string;
  occurTime: isoDatetime;
  ids: string[];
  possibleFixes?: MigrationMapRow[][];
};

export class StateChangeError extends MyError implements StateChangeErrorType {
  field: string;
  occurTime: isoDatetime;
  ids: string[];
  possibleFixes?: MigrationMapRow[][];

  constructor(args: StateChangeErrorType) {
    super(args.message ?? "State change error");
    this.field = args.field;
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
  let startKeyTime = startTime ?? "";
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

    const initialRow = ((
      await db.query(stateChangeView.name, {
        reduce: false,
        startkey: [field, startKeyTime?.slice(0, -1) ?? ""],
        endkey: [field, ""],
        descending: true,
        limit: 1,
      })
    ).rows[0] as StateChangeRow) ?? {
      id: "initial_null_state",
      key: [field, "0000-00-00"],
      value: [null, null],
    };
    processRowsLoop: for (let i = 0; i < stateChangeRows.length; i++) {
      const previousRow = i === 0 ? initialRow : stateChangeRows[i - 1];
      const thisRow = stateChangeRows[i];
      if (isEqual(previousRow.value[1], thisRow.value[0])) {
        continue processRowsLoop;
      }
      // use the block times view to determine if two blocks are overlapping or a block is overlapping with a state change
      const blockCheckStart = previousRow.key[1];
      // for duration based blocks, use the time in the id to determine the end time
      const blockCheckEnd =
        [
          thisRow.key[1],
          extractTimeFromId(previousRow.id) ?? "",
          extractTimeFromId(thisRow.id) ?? "",
        ]
          .sort()
          .at(-1) ?? blockCheckStart;
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
        // if there is an overlapping block error then assume all rows checked are bad and skip to next
        while (stateChangeRows[i].key[1] < blockCheckEnd) {
          i++;
          if (i >= stateChangeRows.length) {
            break processRowsLoop;
          }
        }
        continue processRowsLoop;
      }

      // if no blocks are overlapping then this error is recoverable just by changing the lastState value on the offending entry
      if (fix) {
        const oldDoc = (await db.get(thisRow.id)) as EitherDocument;
        const { data } = pullOutData(oldDoc);
        if (isEqual(data.lastState, thisRow.value[0])) {
          await updateDoc({
            db,
            id: thisRow.id,
            payload: { lastState: previousRow.value[1] },
            outputArgs,
          });
          startKeyTime = previousRow.key[1];
          continue refreshFromDbLoop;
        } else {
          const occurTime = thisRow.key[1];
          const problem =
            "Attempted to fix, but an unexpected lastState was encountered";
          const error = new LastStateError({
            message: `${field}, ${occurTime}: ${problem}. ids: [${previousRow.id}, ${thisRow.id}]`,
            ids: [previousRow.id, thisRow.id],
            occurTime: thisRow.key[1],
            field,
          });
          if (failOnError) {
            throw error;
          } else {
            summary.ok = false;
            summary.errors.push(error);
          }
        }
      }
      const occurTime = thisRow.key[1];
      const problem = "lastState does not match actual last state";
      const error = new LastStateError({
        message: `${field} ${occurTime}: ${problem}. ids: [${previousRow.id}, ${thisRow.id}]`,
        ids: [previousRow.id, thisRow.id],
        occurTime: thisRow.key[1],
        field,
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
  const startkey = [field, startTime ?? ""];
  const endkey = [field, endTime ?? HIGH_STRING];
  const blockTimeRows = (
    await db.query(overlappingBlockView.name, {
      reduce: false,
      startkey,
      endkey,
    })
  ).rows as OverlappingBlockRow[];

  // look backward in time to see if currently in a block or not
  const lastBlockChange: MapRow<typeof durationBlockView> =
    (
      await db.query(durationBlockView.name, {
        reduce: false,
        startkey: [field, startTime?.slice(0, -1) ?? ""], // quick and dirty implementation to skip rows actually occurring at startTime
        endkey: [field, ""],
        descending: true,
        limit: 1,
      })
    ).rows[0] ??
    ({
      key: [field, ""],
      value: false, // default not in a block
      id: "initial_state",
    } as MapRow<typeof durationBlockView>);
  const initialOverlappingBlockRow: OverlappingBlockRow = {
    ...lastBlockChange,
    value: lastBlockChange.value ? 1 : -1,
  };

  blockTimeRows.reduce((lastBlock, curr) => {
    if (lastBlock.value === 1 && curr.value !== -1) {
      const occurTime = curr.key[1];
      const problem =
        curr.value === 1
          ? "Block starts within a block"
          : "State changes within a block";
      const error = new OverlappingBlockError({
        message: `${field} ${occurTime}: ${problem}. ids: [${lastBlock.id}, ${curr.id} ]}`,
        occurTime,
        field,
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
      const occurTime = curr.key[1];
      const problem = "Block ends within a block";
      const error = new OverlappingBlockError({
        message: `${field} ${occurTime}: ${problem}. ids: [${lastBlock.id}, ${curr.id} ]}`,
        occurTime,
        field,
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
  }, initialOverlappingBlockRow);

  return summary;
}

import { isoDatetime } from "../time/timeUtils";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { updateDoc } from "../documentControl/updateDoc";
import { OutputArgs } from "../input/outputArgs";
import isEqual from "lodash.isequal";
import { extractTimeFromId } from "../utils/extractTimeFromId";
import { HIGH_STRING } from "../utils/startsWith";
import { checkOverlappingBlocks } from "./overlappingBlocks";
import { StateChangeError, StateChangeErrorType } from "./stateChangeError";

export class LastStateError extends StateChangeError {
  rows: [StateChangeRow, StateChangeRow];
  constructor(
    args: StateChangeErrorType & { rows: [StateChangeRow, StateChangeRow] },
  ) {
    super(args);
    this.rows = args.rows;
    Object.setPrototypeOf(this, LastStateError.prototype);
  }
}

export type CheckStateType = {
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

export type StateChangeRow = MapRow<typeof stateChangeView>;

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
  let startKeyTime = startTime ?? "";
  const endKeyTime = endTime ?? HIGH_STRING;

  const summary: StateErrorSummary = {
    ok: true,
    errors: [],
  };

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
    stateChangeRows.unshift(initialRow);

    processRowsLoop: while (stateChangeRows.length > 1) {
      let error: LastStateError;
      try {
        const innerSummary = checkStateChangeRows({
          rows: stateChangeRows,
          failOnError: true,
        });
        return innerSummary;
      } catch (e) {
        if (!(e instanceof LastStateError)) {
          throw e;
        }
        error = e;
      }
      const rows = error.rows;

      // use the block times view to determine if two blocks are overlapping or a block is overlapping with a state change
      const [previousRow, thisRow] = rows;
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
        const newStart = stateChangeRows.findIndex(
          (row) => row.key[1] >= blockCheckEnd,
        );
        if (newStart === -1) {
          break;
        }
        stateChangeRows.splice(0, newStart);
        continue processRowsLoop;
      }

      // if no blocks are overlapping then this error is recoverable just by changing the lastState value on the offending entry
      // TODO: add the fix directly into the error for streamlining fixing
      if (fix) {
        await updateDoc({
          db,
          id: thisRow.id,
          payload: { lastState: previousRow.value[1] },
          outputArgs,
        });
        startKeyTime = previousRow.key[1];
        continue refreshFromDbLoop;
      }

      if (failOnError) {
        throw error;
      }
      summary.ok = false;
      summary.errors.push(error);

      const newStart = stateChangeRows.findIndex(
        (row) => row.key[1] >= thisRow.key[1],
      );
      stateChangeRows.splice(0, newStart);
      continue processRowsLoop;
    }
    return summary;
  }
  return summary;
}

export function checkStateChangeRows({
  rows,
  failOnError,
}: {
  rows: StateChangeRow[];
  failOnError?: boolean;
}): StateErrorSummary {
  const summary: StateErrorSummary = {
    ok: true,
    errors: [],
  };
  for (let i = 1; i < rows.length; i++) {
    const previousRow = rows[i - 1];
    const thisRow = rows[i];
    if (isEqual(previousRow.value[1], thisRow.value[0])) {
      continue;
    }
    const field = thisRow.key[0];
    const occurTime = thisRow.key[1];
    const problem = "lastState does not match actual last state";
    const error = new LastStateError({
      message: `${field} ${occurTime}: ${problem}. ids: [${previousRow.id}, ${thisRow.id}]`,
      ids: [previousRow.id, thisRow.id],
      rows: [previousRow, thisRow],
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

import { HIGH_STRING } from "../utils/startsWith";
import { MapRow } from "../views/DatumView";
import { durationBlockView } from "../views/datumViews";
import { overlappingBlockView } from "../views/datumViews/overlappingBlocks";
import {
  CheckStateType,
  StateChangeError,
  StateChangeErrorType,
  StateErrorSummary,
} from "./checkState";

type OverlappingBlockRow = MapRow<typeof overlappingBlockView>;
export class OverlappingBlockError extends StateChangeError {
  constructor(args: StateChangeErrorType) {
    super(args);
    Object.setPrototypeOf(this, OverlappingBlockError.prototype);
  }
}
export async function checkOverlappingBlocks({
  db,
  field,
  startTime,
  endTime,
  failOnError,
}: CheckStateType): Promise<StateErrorSummary> {
  failOnError ??= true;
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

  const summary = checkOverlappingBlockRows({
    rows: [initialOverlappingBlockRow, ...blockTimeRows],
    failOnError,
  });

  return summary;
}

export function checkOverlappingBlockRows({
  rows,
  failOnError,
}: {
  rows: OverlappingBlockRow[];
  failOnError?: boolean;
}): StateErrorSummary {
  let lastBlockChange = rows[0];
  const summary: StateErrorSummary = {
    ok: true,
    errors: [],
  };
  for (let i = 1; i < rows.length; i++) {
    const curr = rows[i];
    const field = curr.key[0];
    if (lastBlockChange.value === 1 && curr.value !== -1) {
      const occurTime = curr.key[1];
      const problem =
        curr.value === 1
          ? "Block starts within a block"
          : "State changes within a block";
      const error = new OverlappingBlockError({
        message: `${field} ${occurTime}: ${problem}. ids: [${lastBlockChange.id}, ${curr.id} ]}`,
        occurTime,
        field,
        ids: [lastBlockChange.id, curr.id],
      });
      if (failOnError) {
        throw error;
      } else {
        summary.ok = false;
        summary.errors.push(error);
      }
    }

    if (lastBlockChange.value === -1 && curr.value === -1) {
      const occurTime = curr.key[1];
      const problem = "Block ends within a block";
      const error = new OverlappingBlockError({
        message: `${field} ${occurTime}: ${problem}. ids: [${lastBlockChange.id}, ${curr.id} ]}`,
        occurTime,
        field,
        ids: [lastBlockChange.id, curr.id],
      });
      if (failOnError) {
        throw error;
      } else {
        summary.ok = false;
        summary.errors.push(error);
      }
    }
    if (curr.value !== 0) {
      lastBlockChange = curr;
    }
  }
  return summary;
}

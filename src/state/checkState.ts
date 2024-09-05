import { isoDatetime } from "../time/timeUtils";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { EitherDocument } from "../documentControl/DatumDocument";
import { durationBlockView } from "../views/datumViews";

type CheckStateType = {
  db: PouchDB.Database;
  field: string;
  startTime?: isoDatetime;
};

export async function checkState({
  db,
  field,
  startTime,
}: CheckStateType): Promise<boolean> {
  type StateChangeRow = MapRow<typeof stateChangeView>;
  const startkey = [field, startTime ?? "0000-00-00"];
  const endkey = [field, "9999-99-99"];

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
    await determineAndThrowStateChangeError(db, context);
    throw new Error("Unreachable--you found a bug");
  }

  return true;
}

async function determineAndThrowStateChangeError(
  db: PouchDB.Database,
  context: MapRow<typeof stateChangeView>[],
): Promise<never> {
  // this should always be called with context[0] being the last good state change, and context[1] reflecting some incorrect previous state
  if (context[0].value[1] === context[1].value[0]) {
    throw new Error(
      "determineAndThrowStateChangeError called when no state change error has occured",
    );
  }

  const ids = context.map((row) => row.id);
  const docs = (await Promise.all(
    ids.map((id) => db.get(id)),
  )) as EitherDocument[];
  context = context.map((row, i) => ({ ...row, doc: docs[i] }));

  // use the block times view to determine if two blocks are overlapping
  const startkey = context[0].key;
  const endkey = context[context.length - 1].key;
  const blockTimeRows = (
    await db.query(durationBlockView.map, {
      reduce: false,
      startkey,
      endkey,
    })
  ).rows as MapRow<typeof durationBlockView>[];
  blockTimeRows
    .map((row) => row.value)
    .reduce((prev, curr) => {
      // must alternate between true and false, otherwise there is an overlap
      if (curr !== !prev) {
        throw new Error("Overlapping blocks");
      }
      return curr;
    });

  // if no blocks are overlapping then this error is recoverable just by changing the lastState value on the offending entry
  throw new Error("incorrect lastState");
}

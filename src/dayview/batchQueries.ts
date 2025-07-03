import { HIGH_STRING } from "../utils/startsWith";
import { stateChangeView } from "../views/datumViews";
import { pointDataView } from "../views/datumViews/pointDataView";
import { DatumState } from "../state/normalizeState";

export type BatchQueryResult = {
  blockRows: StateChangeRow[];
  pointRows: PointDataRow[];
  initialState: DatumState;
};

export type StateChangeRow = {
  key: [string, string];
  value: [string, DatumState];
};

export type PointDataRow = {
  key: [string, string];
  value: any;
};

export type BatchedFieldData = Map<string, BatchQueryResult>;

export async function batchFieldQueries(
  db: PouchDB.Database,
  fields: string[],
  startUtc: string,
  endUtc: string,
): Promise<BatchedFieldData> {
  const result = new Map<string, BatchQueryResult>();

  if (fields.length === 0) {
    return result;
  }

  const [allBlockRows, allPointRows, allInitialStateRows] = await Promise.all([
    db.query(stateChangeView.name, {
      reduce: false,
      startkey: [fields[0], startUtc ?? ""],
      endkey: [fields[fields.length - 1], endUtc ?? HIGH_STRING],
    }),
    db.query(pointDataView.name, {
      reduce: false,
      startkey: [fields[0], startUtc ?? ""],
      endkey: [fields[fields.length - 1], endUtc ?? HIGH_STRING],
    }),
    Promise.all(
      fields.map(async (field) => {
        const rows = await db.query(stateChangeView.name, {
          reduce: false,
          startkey: [field, startUtc?.slice(0, -1) ?? ""],
          endkey: [field, ""],
          descending: true,
          limit: 1,
        });
        return { field, initialState: rows.rows[0]?.value[1] ?? null };
      }),
    ),
  ]);

  const blockRowsByField = new Map<string, StateChangeRow[]>();
  const pointRowsByField = new Map<string, PointDataRow[]>();
  const initialStateByField = new Map<string, DatumState>();

  for (const field of fields) {
    blockRowsByField.set(field, []);
    pointRowsByField.set(field, []);
  }

  for (const row of allBlockRows.rows as StateChangeRow[]) {
    const field = row.key[0];
    if (blockRowsByField.has(field)) {
      blockRowsByField.get(field)!.push(row);
    }
  }

  for (const row of allPointRows.rows as PointDataRow[]) {
    const field = row.key[0];
    if (pointRowsByField.has(field)) {
      pointRowsByField.get(field)!.push(row);
    }
  }

  for (const { field, initialState } of allInitialStateRows) {
    initialStateByField.set(field, initialState);
  }

  for (const field of fields) {
    result.set(field, {
      blockRows: blockRowsByField.get(field) || [],
      pointRows: pointRowsByField.get(field) || [],
      initialState: initialStateByField.get(field) || null,
    });
  }

  return result;
}

export const queryCache = new Map<string, BatchedFieldData>();

export function getCacheKey(
  fields: string[],
  startUtc: string,
  endUtc: string,
): string {
  return `${fields.slice().sort().join(",")}_${startUtc}_${endUtc}`;
}

export async function getCachedFieldData(
  db: PouchDB.Database,
  fields: string[],
  startUtc: string,
  endUtc: string,
): Promise<BatchedFieldData> {
  const cacheKey = getCacheKey(fields, startUtc, endUtc);

  if (queryCache.has(cacheKey)) {
    return queryCache.get(cacheKey)!;
  }

  const result = await batchFieldQueries(db, fields, startUtc, endUtc);
  queryCache.set(cacheKey, result);

  if (queryCache.size > 10) {
    const firstKey = queryCache.keys().next().value;
    if (firstKey) {
      queryCache.delete(firstKey);
    }
  }

  return result;
}

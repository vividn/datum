import { DateTime } from "luxon";
import { isoDatetime } from "../time/timeUtils";

export const LAST_DOCS_ID = "_local/datum_last" as const;

export type LastDocsRef = {
  _id: typeof LAST_DOCS_ID;
  _rev?: string;
  ids: string[];
  time: isoDatetime;
};

export async function updateLastDocsRef(
  db: PouchDB.Database,
  ids: string | string[],
): Promise<void> {
  const idArray = Array.isArray(ids) ? ids : [ids];
  let _rev: string | undefined;
  try {
    ({ _rev } = await db.get(LAST_DOCS_ID));
  } catch (e) {
    //pass
  }
  const lastDocsRef: LastDocsRef = {
    _id: LAST_DOCS_ID,
    _rev,
    ids: idArray,
    time: DateTime.utc().toISO(),
  };

  await db.put(lastDocsRef);
}

export async function getLastDocs(db: PouchDB.Database): Promise<LastDocsRef> {
  const lastDocs: LastDocsRef = await db.get(LAST_DOCS_ID);
  return lastDocs;
}

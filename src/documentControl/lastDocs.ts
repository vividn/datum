import { isCouchDbError } from "../errors";

export const LAST_DOCS_ID = "_local/datum_last" as const;

export type LastDocsRef = {
  _id: typeof LAST_DOCS_ID;
  ids: string[];
};

export async function updateLastDocsRef(
  db: PouchDB.Database,
  ids: string | string[]
): Promise<void> {
  const idArray = Array.isArray(ids) ? ids : [ids];
  let _rev: string | undefined;
  try {
    ({ _rev } = await db.get(LAST_DOCS_ID));
  } catch (e) {
    if (!(isCouchDbError(e) && ["missing", "deleted"].includes(e.reason))) {
      throw e;
    }
  }
  await db.put({
    _id: LAST_DOCS_ID,
    _rev,
    ids: idArray,
  });
}

export async function getLastDocs(db: PouchDB.Database): Promise<LastDocsRef> {
  const lastDocs: LastDocsRef = await db.get(LAST_DOCS_ID);
  return lastDocs;
}

import { DateTime } from "luxon";
import { MyError } from "../errors";
import { isoDatetime } from "../time/timeUtils";

export const LAST_DOCS_ID = "_local/datum_last" as const;

export type LastDocsRef = {
  _id: typeof LAST_DOCS_ID;
  _rev?: string;
  ids: string[];
  time: isoDatetime;
};


export class NoLastDocsRefError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, NoLastDocsRefError.prototype);
  }
}

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
  const lastDocs = (await db.get(LAST_DOCS_ID).catch((e) => {
    if (["missing", "deleted"].includes(e.reason)) {
      throw new NoLastDocsRefError(
        "No reference of last documents exist. Add or change a document first",
      );
    } else {
      throw e;
    }
  })) as LastDocsRef;
  return lastDocs;
}

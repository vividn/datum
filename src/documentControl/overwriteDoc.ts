import { IdError, MyError } from "../errors";
import { DateTime } from "luxon";
import { jClone } from "../utils/jClone";
import isEqual from "lodash.isequal";
import unset from "lodash.unset";
import { BaseDocControlArgs, DocExistsError } from "./base";
import {
  Show,
  showExists,
  showFailed,
  showNoDiff,
  showOWrite,
  showRename,
} from "../output/output";
import { assembleId } from "../ids/assembleId";
import { EitherDocument, EitherPayload } from "./DatumDocument";

function isEquivalent(payload: EitherPayload, existingDoc: EitherDocument) {
  const payloadClone = jClone(payload);
  const docClone = jClone(existingDoc) as EitherPayload;

  // _rev and modifyTime don't matter
  unset(payloadClone, "_rev");
  unset(docClone, "_rev");
  unset(payloadClone, "meta.modifyTime");
  unset(docClone, "meta.modifyTime");

  return isEqual(payloadClone, docClone);
}

type overwriteDocType = {
  id: string;
  payload: EitherPayload;
} & BaseDocControlArgs;

export class OverwriteDocError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, OverwriteDocError.prototype);
  }
}

export class NoDocToOverwriteError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, NoDocToOverwriteError.prototype);
  }
}

export async function overwriteDoc({
  db,
  id,
  payload,
  show = Show.None,
}: overwriteDocType): Promise<EitherDocument> {
  payload = jClone(payload);
  const oldDoc = await db.get(id).catch((e) => {
    if (["missing", "deleted"].includes(e.reason)) {
      throw new NoDocToOverwriteError(
        "doc at id specified to overwrite does not exist"
      );
    } else {
      throw e;
    }
  });

  if (payload._rev !== undefined && oldDoc._rev !== payload._rev) {
    throw new OverwriteDocError("_rev does not match document to overwrite");
  }

  if (payload.meta) {
    const now = DateTime.utc().toString();
    payload.meta.modifyTime = now;
    if (oldDoc.meta?.createTime) {
      payload.meta.createTime = oldDoc.meta.createTime;
    }
  }

  let newId: string;
  try {
    newId = assembleId({ payload });
  } catch (e) {
    if (e instanceof IdError) {
      newId = id;
    } else {
      throw e;
    }
  }

  payload._id = newId;

  if (newId === id) {
    if (isEquivalent(payload, oldDoc)) {
      showNoDiff(oldDoc, show);
      return oldDoc;
    }
    payload._rev = oldDoc._rev;
    await db.insert(payload);
  } else {
    delete payload._rev;
    await db.insert(payload).catch(async (e) => {
      if (e.error === "conflict") {
        const existingDoc = await db.get(newId);
        showExists(existingDoc, show);
        showFailed(payload, show);
        throw new DocExistsError(payload, existingDoc);
      } else {
        throw e;
      }
    });
    await db.destroy(id, oldDoc._rev);
    showRename(id, newId, show);
  }

  const newDoc = await db.get(newId);
  showOWrite(oldDoc, newDoc, show);
  return newDoc;
}

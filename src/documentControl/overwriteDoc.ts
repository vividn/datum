import { IdError, MyError } from "../errors";
import { jClone } from "../utils/jClone";
import isEqual from "lodash.isequal";
import unset from "lodash.unset";
import { BaseDocControlArgs, DocExistsError } from "./base";
import {
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

  // _rev doesn't matter
  unset(payloadClone, "_rev");
  unset(docClone, "_rev");

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
  outputArgs = {},
}: overwriteDocType): Promise<EitherDocument> {
  payload = jClone(payload);
  const oldDoc = await db.get(id).catch((e) => {
    if (["missing", "deleted"].includes(e.reason)) {
      throw new NoDocToOverwriteError(
        "doc at id specified to overwrite does not exist",
      );
    } else {
      throw e;
    }
  });

  if (payload._rev !== undefined && oldDoc._rev !== payload._rev) {
    throw new OverwriteDocError("_rev does not match document to overwrite");
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
      showNoDiff(oldDoc, outputArgs);
      return oldDoc;
    }
    payload._rev = oldDoc._rev;
    await db.put(payload);
  } else {
    delete payload._rev;
    await db.put(payload).catch(async (e) => {
      if (e.name === "conflict") {
        const existingDoc = await db.get(newId);
        showExists(existingDoc, outputArgs);
        showFailed(payload, outputArgs);
        throw new DocExistsError(payload, existingDoc);
      } else {
        throw e;
      }
    });
    await db.remove(id, oldDoc._rev);
    showRename(id, newId, outputArgs);
  }

  const newDoc = await db.get(newId);
  showOWrite(oldDoc, newDoc, outputArgs);
  return newDoc;
}

import {
  DataOnlyPayload,
  EitherDocument,
  EitherPayload,
  isDatumDocument,
  isDatumPayload,
} from "./DatumDocument";
import { combineData, UpdateStrategyNames } from "./combineData";
import { jClone } from "../utils/jClone";
import { IdError, MyError } from "../errors";
import { DateTime } from "luxon";
import {
  showExists,
  showFailed,
  showNoDiff,
  showRename,
  showUpdate,
} from "../output/output";
import isEqual from "lodash.isequal";
import { BaseDocControlArgs, DocExistsError } from "./base";
import { assembleId } from "../ids/assembleId";

export class UpdateDocError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, UpdateDocError.prototype);
  }
}

export class NoDocToUpdateError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, NoDocToUpdateError.prototype);
  }
}

type updateDocType = {
  id: string;
  payload: EitherPayload;
  updateStrategy?: UpdateStrategyNames;
} & BaseDocControlArgs;

export async function updateDoc({
  db,
  id,
  payload,
  updateStrategy = "merge",
  outputArgs = {},
}: updateDocType): Promise<EitherDocument> {
  payload = jClone(payload);
  const oldDoc: EitherDocument = await db.get(id).catch((e) => {
    if (["missing", "deleted"].includes(e.reason)) {
      throw new NoDocToUpdateError(
        "doc at id specified to update does not exist"
      );
    } else {
      throw e;
    }
  });

  if (payload._rev !== undefined && oldDoc._rev !== payload._rev) {
    throw new UpdateDocError("_rev does not match document to update");
  }

  const newData = isDatumPayload(payload) ? payload.data : payload;

  let updatedPayload: EitherPayload;
  if (isDatumDocument(oldDoc)) {
    const oldData = oldDoc.data;
    const updatedData = combineData(oldData, newData, updateStrategy);
    if (isEqual(oldData, updatedData)) {
      showNoDiff(oldDoc, outputArgs);
      return oldDoc;
    }
    const meta = oldDoc.meta;
    meta.modifyTime = DateTime.utc().toString();
    updatedPayload = { data: updatedData, meta: meta };
  } else {
    const oldData = jClone(oldDoc) as DataOnlyPayload;
    delete oldData._rev;
    updatedPayload = combineData(oldData, newData, updateStrategy);

    const updatedPayloadWithDefaultId = { _id: id, ...updatedPayload };
    if (isEqual(oldData, updatedPayloadWithDefaultId)) {
      showNoDiff(oldDoc, outputArgs);
      return oldDoc;
    }
  }

  let newId: string;
  try {
    newId = assembleId({ payload: updatedPayload });
  } catch (e) {
    if (e instanceof IdError) {
      newId = id;
    } else {
      throw e;
    }
  }
  updatedPayload._id = newId;

  if (newId === id) {
    updatedPayload._rev = oldDoc._rev;
    await db.put(updatedPayload);
  } else {
    delete updatedPayload._rev;
    await db.put(updatedPayload).catch(async (e) => {
      if (e.error === "conflict") {
        const existingDoc = await db.get(newId);
        showExists(existingDoc, outputArgs);
        showFailed(updatedPayload, outputArgs);
        throw new DocExistsError(updatedPayload, existingDoc);
      } else {
        throw e;
      }
      ``;
    });
    await db.remove(id, oldDoc._rev);
    showRename(id, newId, outputArgs);
  }

  const newDoc = await db.get(newId);
  showUpdate(oldDoc, newDoc, outputArgs);
  return newDoc;
}

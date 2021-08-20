import {
  EitherDocument,
  EitherPayload,
  isDatumDocument,
  isDatumPayload,
} from "./DatumDocument";
import { DocumentScope } from "nano";
import { DateTime } from "luxon";
import { assembleId } from "../ids";
import { IdError, MyError } from "../errors";
import jClone from "../utils/jClone";
import { UpdateStrategyNames } from "./combineData";
import updateDoc from "./updateDoc";
import { showCreate, showExists, showFailed } from "../output";
import { DocExistsError } from "./base";
import isEqual from "lodash.isequal";

export class AddDocError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, AddDocError.prototype);
  }
}

type addDocType = {
  db: DocumentScope<EitherPayload>;
  payload: EitherPayload;
  conflictStrategy?: UpdateStrategyNames;
  showOutput?: boolean;
  showAll?: boolean;
};

const addDoc = async ({
  db,
  payload,
  conflictStrategy,
  showOutput,
  showAll,
}: addDocType): Promise<EitherDocument> => {
  payload = jClone(payload);
  let id;
  if (isDatumPayload(payload)) {
    const now = DateTime.utc().toString();
    payload.meta.createTime = now;
    payload.meta.modifyTime = now;

    id = assembleId({ payload });

    payload._id = id;

    // Don't care about _rev for adding, will detect and redirect conflicts manually
    delete payload._rev;
  } else {
    id = payload._id;
    if (id === undefined) {
      throw new IdError("id could not be determined");
    }
  }
  try {
    await db.insert(payload);
  } catch (e) {
    if (e.error === "conflict") {
      const existingDoc = await db.get(id);
      const existingWithoutRev = jClone(existingDoc) as EitherPayload;
      delete existingWithoutRev._rev;

      if (conflictStrategy === undefined) {
        // Don't fail if added doc would have had the same data anyway
        if (
          isDatumPayload(payload) &&
          isDatumDocument(existingDoc) &&
          isEqual(payload.data, existingDoc.data)
        ) {
          if (showOutput) {
            showExists(existingDoc, showAll);
          }
        } else if (
          !isDatumPayload(payload) &&
          !isDatumDocument(existingDoc) &&
          isEqual(payload, existingWithoutRev)
        ) {
          if (showOutput) {
            showExists(existingDoc, showAll);
          }
        } else {
          if (showOutput) {
            showExists(existingDoc, showAll);
            showFailed(payload, showAll);
          }
          throw new DocExistsError(payload, existingDoc);
        }
      }
      const updatedDoc = await updateDoc({
        db,
        id,
        payload,
        updateStrategy: conflictStrategy,
        showOutput,
        showAll
      });
      return updatedDoc;
    }
    throw e;
  }
  const addedDoc = await db.get(id);
  if (showOutput) {
    showCreate(addedDoc, showAll);
  }
  return addedDoc;
};

export default addDoc;

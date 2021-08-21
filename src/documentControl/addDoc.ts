import {
  EitherDocument,
  EitherPayload,
  isDatumDocument,
  isDatumPayload,
} from "./DatumDocument";
import { DocumentScope } from "nano";
import { DateTime } from "luxon";
import { assembleId } from "../ids";
import { IdError } from "../errors";
import jClone from "../utils/jClone";
import { UpdateStrategyNames } from "./combineData";
import updateDoc from "./updateDoc";
import { showCreate, showExists, showFailed } from "../output";
import { BaseDocControlArgs, DocExistsError } from "./base";
import isEqual from "lodash.isequal";

function payloadMatchesDbData(
  payload: EitherPayload,
  existingDoc: EitherDocument
) {
  const existingWithoutRev = jClone(existingDoc) as EitherPayload;
  delete existingWithoutRev._rev;

  return (
    (isDatumPayload(payload) &&
      isDatumDocument(existingDoc) &&
      isEqual(payload.data, existingDoc.data)) ||
    (!isDatumPayload(payload) &&
      !isDatumDocument(existingDoc) &&
      isEqual(payload, existingWithoutRev))
  );
}

type addDocType = {
  payload: EitherPayload;
  conflictStrategy?: UpdateStrategyNames;
} & BaseDocControlArgs;

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
    if (e.error !== "conflict") {
      throw e;
    }
    const existingDoc = await db.get(id);

    if (conflictStrategy !== undefined) {
      const updatedDoc = await updateDoc({
        db,
        id,
        payload,
        updateStrategy: conflictStrategy,
        showOutput,
        showAll,
      });
      return updatedDoc;
    }
    if (payloadMatchesDbData(payload, existingDoc)) {
      if (showOutput) {
        showExists(existingDoc, showAll);
      }
      return existingDoc;
    } else {
      if (showOutput) {
        showExists(existingDoc, showAll);
        showFailed(payload, showAll);
      }
      throw new DocExistsError(payload, existingDoc);
    }
  }
  const addedDoc = await db.get(id);
  if (showOutput) {
    showCreate(addedDoc, showAll);
  }
  return addedDoc;
};

export default addDoc;

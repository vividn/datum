import {
  EitherDocument,
  EitherPayload,
  isDatumDocument,
  isDatumPayload,
} from "./DatumDocument";
import { DateTime } from "luxon";
import { assembleId } from "../ids";
import { IdError, isCouchDbError } from "../errors";
import jClone from "../utils/jClone";
import { UpdateStrategyNames } from "./combineData";
import updateDoc from "./updateDoc";
import { Show, showCreate, showExists, showFailed } from "../output";
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
  show = Show.None,
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
  } catch (error) {
    if (isCouchDbError(error) && error.error !== "conflict") {
      throw error;
    }
    const existingDoc = await db.get(id);

    if (conflictStrategy !== undefined) {
      const updatedDoc = await updateDoc({
        db,
        id,
        payload,
        updateStrategy: conflictStrategy,
        show,
      });
      return updatedDoc;
    }
    if (payloadMatchesDbData(payload, existingDoc)) {
      showExists(existingDoc, show);
      return existingDoc;
    } else {
      showExists(existingDoc, show);
      showFailed(payload, show);
      throw new DocExistsError(payload, existingDoc);
    }
  }
  const addedDoc = await db.get(id);
  showCreate(addedDoc, show);
  return addedDoc;
};

export default addDoc;

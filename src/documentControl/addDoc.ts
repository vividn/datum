import {
  EitherDocument,
  EitherPayload,
  isDatumDocument,
  isDatumPayload,
} from "./DatumDocument";
import { DateTime } from "luxon";
import { IdError, isCouchDbError } from "../errors";
import { jClone } from "../utils/jClone";
import { UpdateStrategyNames } from "./combineData";
import { updateDoc } from "./updateDoc";
import { showCreate, showExists, showFailed } from "../output/output";
import { BaseDocControlArgs, DocExistsError } from "./base";
import isEqual from "lodash.isequal";
import { overwriteDoc } from "./overwriteDoc";
import { deleteDoc } from "./deleteDoc";
import {
  DataOrDesignDocument,
  isViewDocument,
  isViewPayload,
  ViewPayload,
} from "../views/DatumView";
import { assembleId } from "../ids/assembleId";
import { toDatumTime } from "../time/datumTime";

function payloadMatchesDbData(
  payload: EitherPayload,
  existingDoc: EitherDocument,
) {
  const existingWithoutRev = jClone(existingDoc) as EitherPayload;
  delete existingWithoutRev._rev;

  return (
    (isDatumPayload(payload) &&
      isDatumDocument(existingDoc) &&
      isEqual(payload.data, existingDoc.data)) ||
    (isViewPayload(payload) &&
      isViewDocument(existingDoc) &&
      isEqual(payload.views, existingDoc.views)) ||
    (!isDatumPayload(payload) &&
      !isDatumDocument(existingDoc) &&
      isEqual(payload, existingWithoutRev))
  );
}

export type ConflictStrategyNames =
  | UpdateStrategyNames
  | "overwrite"
  | "delete"
  | "fail";

const conflictRecord: Record<ConflictStrategyNames, any> = {
  merge: "",
  useOld: "",
  preferOld: "",
  update: "",
  preferNew: "",
  useNew: "",
  removeConflicting: "",
  xor: "",
  intersection: "",
  append: "",
  prepend: "",
  appendSort: "",
  mergeSort: "",
  overwrite: "",
  delete: "",
  fail: "",
};
export const conflictChoices = Object.keys(conflictRecord);

type addDocType = {
  payload: EitherPayload | ViewPayload;
  conflictStrategy?: ConflictStrategyNames;
} & BaseDocControlArgs;

export async function addDoc({
  db,
  payload,
  conflictStrategy,
  outputArgs = {},
}: addDocType): Promise<DataOrDesignDocument> {
  payload = jClone(payload);
  let id;
  if (payload.meta) {
    const now = toDatumTime(DateTime.local());
    payload.meta.createTime = now;
    payload.meta.modifyTime = now;

    // reassemble id in case it depends upon createTime or modifyTime
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
    await db.put(payload);
  } catch (error) {
    if (isCouchDbError(error) && error.name !== "conflict") {
      throw error;
    }
    const existingDoc = await db.get(id);

    if (conflictStrategy !== undefined && conflictStrategy !== "fail") {
      if (conflictStrategy === "overwrite") {
        return overwriteDoc({ db, id, payload, outputArgs: outputArgs });
      }
      if (conflictStrategy === "delete") {
        return deleteDoc({ db, id, outputArgs: outputArgs });
      }
      return await updateDoc({
        db,
        id,
        payload,
        updateStrategy: conflictStrategy,
        outputArgs: outputArgs,
      });
    }
    if (payloadMatchesDbData(payload, existingDoc)) {
      showExists(existingDoc, outputArgs);
      return existingDoc;
    } else {
      showFailed(payload, outputArgs);
      showExists(existingDoc, outputArgs);
      throw new DocExistsError(payload, existingDoc);
    }
  }
  const addedDoc = await db.get(id);
  showCreate(addedDoc, outputArgs);
  return addedDoc;
}

import { DocumentScope } from "nano";
import { EitherDocument, EitherPayload, isDatumPayload } from "./DatumDocument";
import { MyError } from "../errors";
import { assembleId } from "../ids";
import { DateTime } from "luxon";
import jClone from "../utils/jClone";

type overwriteDocType = {
  db: DocumentScope<EitherPayload>;
  id: string;
  payload: EitherPayload;
};

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

const overwriteDoc = async ({
  db,
  id,
  payload,
}: overwriteDocType): Promise<EitherDocument> => {
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

  let newId;
  if (isDatumPayload(payload)) {
    const now = DateTime.utc().toString();
    payload.meta.modifyTime = now;
    if (oldDoc.meta?.createTime) {
      payload.meta.createTime = oldDoc.meta.createTime;
    }

    const calculatedId = payload.meta.idStructure
      ? assembleId({ data: payload.data, meta: payload.meta })
      : payload._id;
    newId = calculatedId ?? id;
  } else {
    newId = payload._id ?? id;
  }

  payload._id = newId;

  if (newId === id) {
    payload._rev = oldDoc._rev;
    await db.insert(payload);
  } else {
    delete payload._rev;
    await db.insert(payload).catch((e) => {
      if (e.error === "conflict") {
        throw new OverwriteDocError("id conflict with another document");
      } else {
        throw e;
      }
    });
    await db.destroy(id, oldDoc._rev);
  }

  const newDoc = await db.get(newId);
  return newDoc;
};

export default overwriteDoc;

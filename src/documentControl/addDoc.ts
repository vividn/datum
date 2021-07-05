import {
  DataOnlyDocument,
  DatumDocument,
  EitherDocument,
  EitherPayload,
  isDatumPayload,
} from "./DatumDocument";
import { DocumentScope } from "nano";
import { DateTime } from "luxon";
import { assembleId } from "../ids";
import { IdError } from "../errors";
import jClone from "../utils/jClone";

type addDocType = {
  db: DocumentScope<EitherPayload>;
  payload: EitherPayload;
};

const addDoc = async ({ db, payload }: addDocType): Promise<EitherDocument> => {
  payload = jClone(payload);
  if (isDatumPayload(payload)) {
    const now = DateTime.utc().toString();
    payload.meta.createTime = now;
    payload.meta.modifyTime = now;

    const id = assembleId({ payload });

    payload._id = id;

    // Don't care about _rev for adding, will detect and redirect conflicts manually
    delete payload._rev;

    await db.insert(payload);
    const addedDoc = (await db.get(id)) as DatumDocument;
    return addedDoc;
  } else {
    const id = payload._id;
    if (id === undefined) {
      throw new IdError("id could not be determined");
    }
    await db.insert(payload);
    const addedDoc = (await db.get(id)) as DataOnlyDocument;
    return addedDoc;
  }
};

export default addDoc;

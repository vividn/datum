import {
  DataOnlyDocument,
  DataOnlyPayload,
  DatumDocument,
  DatumPayload,
  isDatumDocument,
  isDatumPayload,
} from "./DatumDocument";
import { DocumentScope } from "nano";
import { DateTime } from "luxon";
import { assembleId } from "../ids";
import { IdError } from "../errors";

type addDocType = {
  db: DocumentScope<DatumPayload | DataOnlyPayload>;
  payload: DatumPayload | DataOnlyPayload;
};

const addDoc = async ({
  db,
  payload,
}: addDocType): Promise<DatumDocument | DataOnlyDocument> => {
  if (isDatumPayload(payload)) {
    const now = DateTime.utc.toString();
    payload.meta.createTime = now;
    payload.meta.modifyTime = now;

    const id = payload.meta.idStructure
      ? assembleId({ data: payload.data, meta: payload.meta })
      : payload._id ?? undefined;
    if (id === undefined) {
      throw new IdError("id could not be determined");
    }
    payload._id = id;

    await db.insert(payload);
    const addedDoc = await db.get(id) as DatumDocument;
    return addedDoc;
  } else {
    return payload as DataOnlyDocument;
  }
};

export default addDoc;

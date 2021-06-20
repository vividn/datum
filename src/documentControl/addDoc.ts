import {
  DataOnlyDocument,
  DataOnlyPayload,
  DatumDocument,
  DatumPayload,
  isDatumDocument,
  isDatumPayload,
} from "./DatumDocument";
import { DocumentScope } from "nano";

type addDocType = {
  db: DocumentScope<DatumDocument | DataOnlyDocument>;
  payload: DatumPayload | DataOnlyPayload;
  id?: string;
};

export default async ({
  db,
  payload,
  id,
}: addDocType): Promise<DatumDocument | DataOnlyDocument> => {
  if (isDatumPayload(payload)) {
    return payload as DatumDocument;
  } else {
    return payload as DataOnlyDocument;
  }
};

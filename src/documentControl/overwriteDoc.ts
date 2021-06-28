import { DocumentScope } from "nano";
import { EitherDocument, EitherPayload } from "./DatumDocument";

type overwriteDocType = {
  db: DocumentScope<EitherPayload>;
  id: string;
  payload: EitherPayload;
}

const overwriteDoc = async ({db, id, payload}: overwriteDocType): Promise<EitherDocument> => {
  return {_id: "failing", _rev: "failing"};
};

export default overwriteDoc;
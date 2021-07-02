import { DocumentScope } from "nano";
import { EitherDocument, EitherPayload } from "./DatumDocument";
import { MyError } from "../errors";

type overwriteDocType = {
  db: DocumentScope<EitherPayload>;
  id: string;
  payload: EitherPayload;
}

export class OverwriteDocError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, OverwriteDocError.prototype);
  }
}

const overwriteDoc = async ({db, id, payload}: overwriteDocType): Promise<EitherDocument> => {
  return {_id: "failing", _rev: "failing"};
};

export default overwriteDoc;
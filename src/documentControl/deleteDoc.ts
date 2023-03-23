import { BaseDocControlArgs } from "./base";
import { isCouchDbError, MyError } from "../errors";
import { showDelete } from "../output/output";
import { EitherDocument, isDatumDocument } from "./DatumDocument";
import { DateTime } from "luxon";

export class NoDocToDeleteError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, NoDocToDeleteError.prototype);
  }
}

type deleteDocType = {
  id: string;
} & BaseDocControlArgs;

export type DeletedDocument = {
  _id: string;
  _rev: string;
  _deleted: true;
};

export async function deleteDoc({
  id,
  db,
  outputArgs = {},
}: deleteDocType): Promise<EitherDocument & { _deleted: true }> {
  let existingDoc;
  try {
    existingDoc = await db.get(id);
  } catch (error) {
    if (isCouchDbError(error) && error.error === "not_found") {
      throw new NoDocToDeleteError(`no document exists with id: ${id}`);
    } else {
      throw error;
    }
  }

  if (isDatumDocument(existingDoc)) {
    existingDoc.meta.modifyTime = DateTime.utc().toString();
  }

  const deletePayload = { ...existingDoc, _deleted: true };
  const { rev: newRev } = await db.put(deletePayload);

  const deletedDoc: DeletedDocument = { ...deletePayload, _rev: newRev };
  showDelete(deletedDoc, outputArgs);
  return deletedDoc;
}

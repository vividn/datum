import { BaseDocControlArgs } from "./base";
import { isCouchDbError, MyError } from "../errors";
import { showDelete } from "../output/output";

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
}: deleteDocType): Promise<DeletedDocument> {
  let existingDoc;
  try {
    existingDoc = await db.get(id);
  } catch (error) {
    if (isCouchDbError(error) && error.name === "not_found") {
      throw new NoDocToDeleteError(`no document exists with id: ${id}`);
    } else {
      throw error;
    }
  }

  const deletedRev = (await db.remove(id, existingDoc._rev)).rev;
  const deletedDoc = (await db.get(id, { rev: deletedRev })) as DeletedDocument;
  showDelete(existingDoc, outputArgs);
  return deletedDoc;
}

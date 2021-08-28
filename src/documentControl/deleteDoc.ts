import { BaseDocControlArgs } from "./base";
import { EitherDocument } from "./DatumDocument";
import { isCouchDbError, MyError } from "../errors";
import { Show, showDelete } from "../output";

export class NoDocToDeleteError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, NoDocToDeleteError.prototype);
  }
}

type deleteDocType = {
  id: string;
} & BaseDocControlArgs;

export async function deleteDoc({
  id,
  db,
  show = Show.None,
}: deleteDocType): Promise<EitherDocument> {
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

  await db.destroy(id, existingDoc._rev);
  showDelete(existingDoc, show);
  return existingDoc;
}

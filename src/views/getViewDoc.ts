import { DocumentScope, ViewDocument } from "nano";

type GetViewDocType = {

}

export async function getViewDoc () {}

export function asViewDb<T> (db: DocumentScope<T>): DocumentScope<ViewDocument<T>> {
  return db as unknown as DocumentScope<ViewDocument<T>>;
}
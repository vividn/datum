import { resetTestDb, testNano } from "../test-utils";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import { afterEach, beforeEach, expect, it } from "@jest/globals";
import deleteDoc, {
  NoDocToDeleteError,
} from "../../src/documentControl/deleteDoc";

const dbName = "delete_doc_test";
const db = testNano.db.use<EitherPayload>(dbName);

beforeEach(async () => {
  await resetTestDb(dbName);
});

afterEach(async () => {
  await testNano.db.destroy(dbName);
});

it("deletes the document with the given in the db", async () => {
  await db.insert({ _id: "doc-to-delete" });
  await deleteDoc({ id: "doc-to-delete", db });
  await expect(db.get("doc-to-delete")).rejects.toThrow("deleted");
});

it("returns a DeletedDocument, with a new _rev", async () => {
  await db.insert({ _id: "someDoc", foo: "bar", baz: "abc" });
  const existingDoc = await db.get("someDoc");

  const deletedDocument = await deleteDoc({ id: "someDoc", db });

  const oldRevNumber = Number(existingDoc._rev.split("-")[0]);
  const newRevNumber = Number(deletedDocument._rev.split("-")[0]);
  expect(newRevNumber).toEqual(oldRevNumber + 1);

  expect(deletedDocument._deleted).toBe(true);
});

it("throws if doc at id does not exist", async () => {
  await expect(deleteDoc({ id: "does-not-exist", db })).rejects.toThrowError(
    NoDocToDeleteError
  );
});

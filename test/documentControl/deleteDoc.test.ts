import { pass, testNano } from "../test-utils";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import { afterEach, beforeAll, beforeEach, expect, it } from "@jest/globals";
import {
  deleteDoc,
  NoDocToDeleteError,
} from "../../src/documentControl/deleteDoc";

const dbName = "delete_doc_test";
const db = testNano.db.use<EitherPayload>(dbName);

beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
});

afterEach(async () => {
  await testNano.db.destroy(dbName);
});

it("deletes the document with the given in the db", async () => {
  await db.insert({ _id: "doc-to-delete" });
  await deleteDoc({ id: "doc-to-delete", db });
  await expect(db.get("doc-to-delete")).rejects.toThrow("deleted");
});

it("returns the deleted document", async () => {
  await db.insert({ _id: "someDoc", foo: "bar", baz: "abc" });
  const existingDoc = await db.get("someDoc");

  const returned = await deleteDoc({ id: "someDoc", db });
  expect(returned).toEqual(existingDoc);
});

it("throws if doc at id does not exist", async () => {
  await expect(deleteDoc({ id: "does-not-exist", db })).rejects.toThrowError(
    NoDocToDeleteError
  );
});

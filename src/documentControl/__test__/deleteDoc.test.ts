import { pass, resetTestDb } from "../../test-utils";
import { deleteDoc, NoDocToDeleteError } from "../deleteDoc";
import { EitherPayload } from "../DatumDocument";

describe("deleteDoc", () => {
  const dbName = "delete_doc_test";
  let db: PouchDB.Database<EitherPayload>;

  beforeEach(async () => {
    db = await resetTestDb(dbName);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  it("deletes the document with the given in the db", async () => {
    await db.put({ _id: "doc-to-delete" });
    await deleteDoc({ id: "doc-to-delete", db });
    await expect(
      db.get("doc-to-delete")
    ).rejects.toThrowErrorMatchingInlineSnapshot(`"deleted"`);
  });

  it("returns a DeletedDocument, with a new _rev", async () => {
    await db.put({ _id: "someDoc", foo: "bar", baz: "abc" });
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
});

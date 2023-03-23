import { at, pass, resetTestDb, restoreNow, setNow } from "../../test-utils";
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

  it("it only adds _deleted, preserving the other fields in the deleted rev", async () => {
    const documentToDelete = { _id: "doc_with_data", foo: "bar", baz: "abc" };
    await db.put(documentToDelete);
    const deletedDocument = await deleteDoc({ id: "doc_with_data", db });
    expect(deletedDocument).toMatchObject({
      ...documentToDelete,
      _deleted: true,
    });

    const deletedDocFromDb = await db.get("doc_with_data", {
      rev: deletedDocument._rev,
    });
    expect(deletedDocFromDb).toMatchObject({
      ...documentToDelete,
      _deleted: true,
    });

    await expect(
      db.get("doc_with_data")
    ).rejects.toThrowErrorMatchingInlineSnapshot(`"deleted"`);
  });

  it("updates modifiedTime on datum documents", async () => {
    const documentToDelete = {
      _id: "doc_with_meta",
      data: { abc: 123 },
      meta: { modifyTime: "2020-01-01T00:00:00.000Z" },
    };
    await db.put(documentToDelete);
    const time = "2023-03-23T20:38:00.000Z";
    setNow(time);
    const deletedDocument = await deleteDoc({ id: "doc_with_meta", db });
    expect(deletedDocument).toMatchObject({
      ...documentToDelete,
      meta: { ...documentToDelete.meta, modifyTime: time },
      _deleted: true,
    });
    restoreNow();
  });
});

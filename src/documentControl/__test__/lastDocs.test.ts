import { addCmd } from "../../commands/addCmd";
import { getLastDocs } from "../lastDocs";
import { testDbLifecycle } from "../../test-utils";
import { deleteCmd } from "../../commands/deleteCmd";
import * as editInTerminal from "../../utils/editInTerminal";
import { GenericObject } from "../../GenericObject";
import { editCmd } from "../../commands/editCmd";
import { setupCmd } from "../../commands/setupCmd";
import { getCmd } from "../../commands/getCmd";
import { endCmd } from "../../commands/endCmd";
import { grepCmd } from "../../commands/grepCmd";

describe("lastDocs", () => {
  const dbName = "last_docs_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd({ db: dbName });
  });
  test("addCmd updates the local last doc reference", async () => {
    const doc = await addCmd({
      idPart: "doc-id-of-last-doc",
      data: ["foo=abc"],
    });
    const lastDocs = await getLastDocs(db);
    expect(lastDocs.ids).toEqual([doc._id]);
  });

  test("deleteCmd updates the local last doc reference", async () => {
    const id = "last-doc-deleted";
    await db.put({ _id: id, foo: "bar" });
    await db.put({ _id: "some-other-doc", foo: "baz" });
    await expect(getLastDocs(db)).rejects.toThrowError();

    await deleteCmd({ quickId: id });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([id]);
  });

  test("editCmd updates lastDocRef", async () => {
    jest
      .spyOn(editInTerminal, "editJSONInTerminal")
      .mockImplementation(async (doc: GenericObject) => doc);
    const id = "last-doc-edited";
    await db.put({ _id: id, foo: "bar" });

    await editCmd({ quickId: id });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([id]);
  });

  test("endCmd updates lastDocRef", async () => {
    const { _id } = await endCmd({ field: "field" });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("getCmd updates lastDocRef", async () => {
    const id = "last-doc-gotten-1";
    await db.put({ _id: id, foo: "bar" });
    await getCmd({ quickId: id });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([id]);
  });

  test("grepCmd updates lastDocRef", async () => {
    await db.bulkDocs([
      { _id: "a", has: "match" },
      { _id: "b", data: { also: "has match" } },
      { _id: "c", data: { does: "not" } },
      { _id: "d", data: {} },
      { _id: "e", match: "has" },
      { _id: "f", data: { match: "also grepped" } },
    ]);
    const docs = await grepCmd({ patterns: ["match"] });
    expect(docs.map((doc) => doc._id)).toEqual(["a", "b", "e", "f"]);
    const lastDocRef = await getLastDocs(db);
    expect(lastDocRef.ids).toEqual(["a", "b", "e", "f"]);
  });
});

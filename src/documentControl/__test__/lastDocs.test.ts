import { addCmd } from "../../commands/addCmd";
import { getLastDocs } from "../lastDocs";
import { testDbLifecycle } from "../../test-utils";
import { deleteCmd } from "../../commands/deleteCmd";

describe("lastDocs", () => {
  const dbName = "last_docs_test";
  const db = testDbLifecycle(dbName);

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
});

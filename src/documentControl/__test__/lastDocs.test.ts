import { addCmd } from "../../commands/addCmd";
import { getLastDocs } from "../lastDocs";
import { testDbLifecycle } from "../../test-utils";
import { deleteCmd } from "../../commands/deleteCmd";
import * as editInTerminal from "../../utils/editInTerminal";
import { GenericObject } from "../../GenericObject";
import { editCmd } from "../../commands/editCmd";
import { setupCmd } from "../../commands/setupCmd";

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
});

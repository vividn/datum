import { addCmd } from "../../commands/addCmd";
import { getLastDocs } from "../lastDocs";
import { setNow, testDbLifecycle } from "../../test-utils";
import { deleteCmd } from "../../commands/deleteCmd";
import * as editInTerminal from "../../utils/editInTerminal";
import { GenericObject } from "../../GenericObject";
import { editCmd } from "../../commands/editCmd";
import { setupCmd } from "../../commands/setupCmd";
import { getCmd } from "../../commands/getCmd";
import { endCmd } from "../../commands/endCmd";
import { grepCmd } from "../../commands/grepCmd";
import { occurCmd } from "../../commands/occurCmd";
import { startCmd } from "../../commands/startCmd";
import { switchCmd } from "../../commands/switchCmd";
import { tailCmd } from "../../commands/tailCmd";
import { updateCmd } from "../../commands/updateCmd";

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
      { _id: "a", has: "matchString" },
      { _id: "b", data: { also: "has matchString" } },
      { _id: "c", data: { does: "not" } },
      { _id: "d", data: {} },
      { _id: "e", matchString: "has" },
      { _id: "f", data: { matchString: "also grepped" } },
    ]);
    const docs = await grepCmd({ patterns: ["matchString"] });
    expect(docs.map((doc) => doc._id)).toEqual(["a", "b", "e", "f"]);
    const lastDocRef = await getLastDocs(db);
    expect(lastDocRef.ids).toEqual(["a", "b", "e", "f"]);
  });

  test("occurCmd updates lastDocRef", async () => {
    const { _id } = await occurCmd({ field: "field" });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("startCmd updates lastDocRef", async () => {
    const { _id } = await startCmd({ field: "field" });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("switchCmd updates lastDocRef", async () => {
    const { _id } = await switchCmd({ field: "field", state: "newState" });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("tailCmd updates lastDocRef", async () => {
    const { _id: _id1 } = await occurCmd({ field: "field" });
    setNow("+1");
    const { _id: _id2 } = await occurCmd({ field: "field2" });
    setNow("+1");
    const { _id: _id3 } = await occurCmd({ field: "field" });
    setNow("+1");
    const { _id: id4 } = await occurCmd({ field: "field2" });
    setNow("+1");
    const { _id: id5 } = await occurCmd({ field: "field" });
    setNow("+1");
    const { _id: id6 } = await occurCmd({ field: "field2" });
    setNow("+1");

    await tailCmd({ num: 3 });
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([id4, id5, id6]);
  });

  test("updateCmd updates lastDocRef", async () => {
    const { _id: firstId } = await addCmd({
      data: ["foo=bar"],
      idPart: "%foo",
      field: "field",
    });
    expect(firstId).toEqual("field:bar");

    const [{ _id: secondId }] = await updateCmd({
      quickId: "field:bar",
      data: ["foo=baz"],
    });
    expect(secondId).toEqual("field:baz");

    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef).toEqual({ ids: [secondId] });
  });
});

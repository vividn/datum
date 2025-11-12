import { addCmd } from "../../commands/addCmd.js";
import { getLastDocs } from "../lastDocs.js";
import { restoreNow, setNow, testDbLifecycle } from "../../__test__/test-utils.js";
import { deleteCmd } from "../../commands/deleteCmd.js";
import * as editInTerminal from "../../utils/editInTerminal.js";
import { editCmd } from "../../commands/editCmd.js";
import { setupCmd } from "../../commands/setupCmd.js";
import { getCmd } from "../../commands/getCmd.js";
import { endCmd } from "../../commands/endCmd.js";
import { grepCmd } from "../../commands/grepCmd.js";
import { occurCmd } from "../../commands/occurCmd.js";
import { startCmd } from "../../commands/startCmd.js";
import { switchCmd } from "../../commands/switchCmd.js";
import { updateCmd } from "../../commands/updateCmd.js";
import { JsonObject } from "../../utils/utilityTypes.js";

describe("lastDocs", () => {
  const dbName = "last_docs_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
  });
  test("addCmd updates the local last doc reference", async () => {
    const doc = await addCmd("--id doc-id-of-last-doc -F foo=abc");
    const lastDocs = await getLastDocs(db);
    expect(lastDocs.ids).toEqual([doc._id]);
  });

  test("deleteCmd updates the local last doc reference", async () => {
    const id = "last-doc-deleted";
    await db.put({ _id: id, foo: "bar" });
    await db.put({ _id: "some-other-doc", foo: "baz" });
    await expect(getLastDocs(db)).rejects.toThrow();

    await deleteCmd(id);
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([id]);
  });

  test("editCmd updates lastDocRef", async () => {
    jest
      .spyOn(editInTerminal, "editJSONInTerminal")
      .mockImplementation(async (doc: JsonObject) => doc);
    const id = "last-doc-edited";
    await db.put({ _id: id, foo: "bar" });

    await editCmd(id);
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([id]);
  });

  test("endCmd updates lastDocRef", async () => {
    const { _id } = await endCmd("field");
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("getCmd updates lastDocRef", async () => {
    const id = "last-doc-gotten-1";
    await db.put({ _id: id, foo: "bar" });
    await getCmd(id);
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
    const docs = await grepCmd("matchString");
    expect(docs.map((doc) => doc._id)).toEqual(["a", "b", "e", "f"]);
    const lastDocRef = await getLastDocs(db);
    expect(lastDocRef.ids).toEqual(["a", "b", "e", "f"]);
  });

  test("occurCmd updates lastDocRef", async () => {
    const { _id } = await occurCmd("field");
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("startCmd updates lastDocRef", async () => {
    const { _id } = await startCmd("field");
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("switchCmd updates lastDocRef", async () => {
    const { _id } = await switchCmd("field newState");
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
  });

  test("updateCmd updates lastDocRef", async () => {
    const { _id: firstId } = await addCmd("field foo=bar --id %foo");
    expect(firstId).toEqual("field:bar");

    const [{ _id: secondId }] = await updateCmd("field:bar foo=baz");
    expect(secondId).toEqual("field:baz");

    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([secondId]);
  });

  test("it includes the time in the lastDocsRef", async () => {
    setNow("2024-05-10, 15:15");
    const { _id } = await addCmd("field foo=bar --id %foo");
    const lastDocsRef = await getLastDocs(db);
    expect(lastDocsRef.ids).toEqual([_id]);
    expect(lastDocsRef.time).toEqual("2024-05-10T15:15:00.000Z");
    restoreNow();
  });
});

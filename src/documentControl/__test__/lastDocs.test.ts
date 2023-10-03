import { addCmd } from "../../commands/addCmd";
import { getLastDocs } from "../lastDocs";
import { testDbLifecycle } from "../../test-utils";

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
});

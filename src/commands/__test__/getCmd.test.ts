import { testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../setupCmd";
import { getCmd } from "../getCmd";
import { Show } from "../../input/outputArgs";

describe("getCmd", () => {
  const dbName = "get_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
  });

  it("gets a document based on the first few letters of humanId", async () => {
    const doc = { _id: "hello", data: {}, meta: { humanId: "a44quickId" } };
    await db.put(doc);
    const retDocs = await getCmd("a44");
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]).toEqual({ ...doc, _rev: expect.anything() });
  });

  it("gets a document based on the first few letters of _id", async () => {
    const doc = { _id: "the_quick_brown_fox", foo: "abc" };
    await db.put(doc);
    const retDocs = await getCmd("the_qu");
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]).toEqual({ ...doc, _rev: expect.anything() });
  });

  it("outputs an EXISTS message when show is standard", async () => {
    const originalLog = console.log;
    const mockLog = jest.fn();
    console.log = mockLog;

    const doc = {
      _id: "show_me_a_message",
      data: {},
      meta: { humanId: "somethingElse" },
    };
    await db.put(doc);
    await getCmd("show_me", { show: Show.Standard });
    expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));

    console.log = originalLog;
  });

  it("can get multiple documents with a compound quickId", async () => {
    const doc1 = { _id: "id1", data: {}, meta: { humanId: "abc" } };
    const doc2 = { _id: "id2", data: {}, meta: { humanId: "def" } };
    await db.put(doc1);
    await db.put(doc2);

    const returned = await getCmd("abc,def,");
    expect(returned).toEqual([
      expect.objectContaining(doc1),
      expect.objectContaining(doc2),
    ]);
  });
});

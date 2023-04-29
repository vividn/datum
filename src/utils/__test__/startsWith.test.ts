import { startsWith } from "../startsWith";
import { testDbLifecycle } from "../../test-utils";

describe("startsWith", () => {
  it.each([
    ["a", "a\uffff\uffff\uffff\uffff"],
    ["asdf", "asdf\uffff\uffff\uffff\uffff"],
    ["zzz", "zzz\uffff\uffff\uffff\uffff"],
    ["ƞ", "ƞ\uffff\uffff\uffff\uffff"],
  ])(
    "returns endkey as the startkey plus a bunch of high value unicode letters for string start keys",
    (startKey, endKey) => {
      expect(startsWith(startKey)).toEqual({
        startkey: startKey,
        endkey: endKey,
      });
    }
  );

  it("returns number+epsilon as the endkey if startkey is a number", () => {
    expect(startsWith(127)).toMatchObject({
      startkey: 127,
      endkey: 127.00000000000001,
    });
    expect(startsWith(3460000)).toMatchObject({
      endkey: 3460000.0000000005,
      startkey: 3460000,
    });
  });

  it("returns array end keys appropriately for arrays", () => {
    expect(startsWith(["abc"])).toEqual({
      startkey: ["abc"],
      endkey: [
        "abc",
        { "\uffff\uffff\uffff\uffff": "\uffff\uffff\uffff\uffff" },
      ],
    });
  });

  it("returns very high object end keys for objects", () => {
    expect(startsWith({ foo: "bar" })).toEqual({
      startkey: { foo: "bar" },
      endkey: {
        "\uffff\uffff\uffff\uffff": "\uffff\uffff\uffff\uffff",
      },
    });
  });

  describe("test with db", () => {
    const dbName = "test_starts_with";
    const db = testDbLifecycle(dbName);

    test("output can be used to filter the list of _all_docs to just docs that start with the string", async () => {
      await db.put({ _id: "aaabc" });
      await db.put({ _id: "aazzz" });
      await db.put({ _id: "aa\ufff0\ufff0\ufff0" }); // Very high codepoint that couchdb recommend to filter (with inclusive_end: true), but should still be included with this superior method
      await db.put({ _id: "ab_not_included" });

      const doc_list = await db.allDocs(startsWith("aa"));
      const ids = doc_list.rows.map((row) => row.id);
      expect(ids.length).toBe(3);
      expect(ids).toEqual(["aaabc", "aazzz", "aa\ufff0\ufff0\ufff0"]);
    });

    it("does startWith arrays for both raw-collated and normal views", async () => {

    });
  });
});

import { describe, expect, it, test } from "@jest/globals";
import inferType from "../src/utils/inferType";
import { splitFirst } from "../src/utils/splitFirst";
import { createOrAppend } from "../src/utils/createOrAppend";
import startsWith from "../src/utils/startsWith";
import { pass, testNano } from "./test-utils";

describe("inferType", () => {
  it("leaves numbers as numbers", () => {
    expect(inferType(3)).toBe(3);
    expect(inferType(-45.5)).toBe(-45.5);
  });

  it("converts strings that are number to numbers", () => {
    expect(inferType("3")).toBe(3);
    expect(inferType("-45.5")).toBe(-45.5);
  });

  it("handles special number strings", () => {
    expect(inferType("nan")).toBe(Number.NaN);
    expect(inferType("NaN")).toBe(Number.NaN);
    expect(inferType("NAN")).toBe(Number.NaN);

    expect(inferType("null")).toBe(null);
    expect(inferType("NULL")).toBe(null);

    expect(inferType("inf")).toBe(Number.POSITIVE_INFINITY);
    expect(inferType("infinity")).toBe(Number.POSITIVE_INFINITY);
    expect(inferType("-inf")).toBe(Number.NEGATIVE_INFINITY);
    expect(inferType("-infinity")).toBe(Number.NEGATIVE_INFINITY);
  });

  it("converts array looking data", () => {
    expect(inferType("[3, 4, 5]")).toEqual([3, 4, 5]);
    expect(inferType("[a, b ,c]")).toEqual(["a", "b", "c"]);
    expect(inferType("[]")).toEqual([]);
    expect(inferType("[a, 3, [mixed, [2], nested]]")).toEqual([
      "a",
      3,
      ["mixed", [2], "nested"],
    ]);
  });

  it("converts JSON looking data", () => {
    expect(inferType("{}")).toEqual({});
    expect(inferType("{a: bcd, d: 3}")).toEqual({ d: 3, a: "bcd" });
    expect(inferType("{turtles: {all: {the: {way: down}}}}")).toEqual({
      turtles: { all: { the: { way: "down" } } },
    });
    expect(inferType("{flat: {earth: [or, 1, turtle, shell]}}")).toEqual({
      flat: { earth: ["or", 1, "turtle", "shell"] },
    });
  });

  it("parses weird looking things as strings", () => {
    expect(inferType("{what: even is this)) ]}")).toEqual(
      "{what: even is this)) ]}"
    );
    expect(inferType("(1,2,3)")).toEqual("(1,2,3)");
    expect(inferType("NAN/null")).toEqual("NAN/null");
  });
});

describe("splitFirst", () => {
  const splitFirstEquals = (str: string) => splitFirst("=", str);
  it("returns [str] if there are no equals signs", () => {
    expect(splitFirstEquals("")).toStrictEqual([""]);
    expect(splitFirstEquals("a")).toStrictEqual(["a"]);
    expect(splitFirstEquals("a,bsdflkj3")).toStrictEqual(["a,bsdflkj3"]);
  });

  it("returns key value pair when there is one equals", () => {
    expect(splitFirstEquals("a=b")).toStrictEqual(["a", "b"]);
    expect(splitFirstEquals("a=")).toStrictEqual(["a", ""]);
    expect(splitFirstEquals("abc=def")).toStrictEqual(["abc", "def"]);
  });

  it("puts any extra equals signs into the value of the pair", () => {
    expect(splitFirstEquals("a=b=c")).toStrictEqual(["a", "b=c"]);
    expect(splitFirstEquals("a====")).toStrictEqual(["a", "==="]);
  });

  it("can also use other separators", () => {
    expect(splitFirst(":", "no_colons_here")).toStrictEqual(["no_colons_here"]);
    expect(splitFirst(":", "a:b:c")).toStrictEqual(["a", "b:c"]);
    expect(splitFirst(":", "a:::::")).toStrictEqual(["a", "::::"]);
    expect(splitFirst("/", "a/b/c")).toStrictEqual(["a", "b/c"]);
  });
});

describe("createOrAppend", () => {
  it("appends if existing is array", () => {
    expect(createOrAppend(["a", 2], "c")).toStrictEqual(["a", 2, "c"]);
    expect(createOrAppend([], "newValue")).toStrictEqual(["newValue"]);
    expect(createOrAppend(["not", "flattened"], [])).toStrictEqual([
      "not",
      "flattened",
      [],
    ]);
  });

  it("creates an array of the two values if existing is a nonarray value", () => {
    expect(createOrAppend("two", "strings")).toStrictEqual(["two", "strings"]);
    expect(
      createOrAppend("leftString", ["right", "side", "array"])
    ).toStrictEqual(["leftString", ["right", "side", "array"]]);
    expect(createOrAppend(1, 2)).toStrictEqual([1, 2]);
    expect(createOrAppend({ a: 123 }, { b: 456 })).toStrictEqual([
      { a: 123 },
      { b: 456 },
    ]);
  });
  it("uses the new value if the existing is undefined", () => {
    expect(createOrAppend(undefined, "right side")).toStrictEqual("right side");
    expect(createOrAppend(undefined, ["a", "b", "c"])).toStrictEqual([
      "a",
      "b",
      "c",
    ]);
  });
});

describe("startsWith", () => {
  it.each([
    ["a", "a\uffff\uffff\uffff\uffff"],
    ["asdf", "asdf\uffff\uffff\uffff\uffff"],
    ["zzz", "zzz\uffff\uffff\uffff\uffff"],
    ["ƞ", "ƞ\uffffuffffuffffuffff"],
  ])(
    "returns endKey as the start key plus a bunch of high value unicode letters",
    (startKey, endKey) => {
      expect(startsWith(startKey)).toEqual({
        start_key: startKey,
        end_key: endKey,
      });
    }
  );

  test("output can be used to filter the list of _all_docs to just docs that start with the string", async () => {
    const dbName = "test_starts_with";
    await testNano.db.destroy(dbName).catch(pass);
    await testNano.db.create(dbName);
    const db = testNano.use(dbName);

    await db.insert({ _id: "aaabc" });
    await db.insert({ _id: "aazzz" });
    await db.insert({ _id: "aa\ufff0\ufff0\ufff0" }); // Very high codepoint that couchdb recommend to filter (with inclusive_end: true), but should still be included with this superior method
    await db.insert({ _id: "ab_not_included" });

    const doc_list = await db.list(startsWith("aa"));
    const ids = doc_list.rows.map((row) => row.id);
    expect(ids.length).toBe(3);
    expect(ids).toEqual(["aaabc", "aazzz", "aa\ufff0\ufff0\ufff0"]);

    await testNano.db.destroy(dbName);
  });
});

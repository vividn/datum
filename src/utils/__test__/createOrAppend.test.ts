import { createOrAppend } from "../createOrAppend";

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

  it("concats nulls into the array", () => {
    expect(createOrAppend(null, "rightside")).toStrictEqual([
      null,
      "rightside",
    ]);
    expect(createOrAppend("leftside", null)).toStrictEqual(["leftside", null]);
  });

  it("can join the output into a string", () => {
    expect(createOrAppend("abc", "def", " ")).toStrictEqual("abc def");
  });
});

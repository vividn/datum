const {inferType, splitFirstEquals} = require('../src/utils')

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
    expect(inferType([3, 4, 5])).toEqual([3, 4, 5]);
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

describe("splitFirstEquals", () => {
  it("returns [str, undefined] if there are no equals signs", () => {
    expect(splitFirstEquals("")).toStrictEqual(["", undefined]);
    expect(splitFirstEquals("a")).toStrictEqual(["a", undefined]);
    expect(splitFirstEquals("a,bsdflkj3")).toStrictEqual([
      "a,bsdflkj3",
      undefined,
    ]);
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
});
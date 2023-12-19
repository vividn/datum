import { inferType } from "../inferType";
import SpyInstance = jest.SpyInstance;
import { setNow } from "../../__test__/test-utils";
import { toDatumTime } from "../../time/timeUtils";
import { BadDateError, BadDurationError, BadTimeError } from "../../errors";

describe("inferType", () => {
  it("leaves numbers as numbers", () => {
    expect(inferType(3)).toBe(3);
    expect(inferType(-45.5)).toBe(-45.5);
  });

  it("converts strings that are number to numbers", () => {
    expect(inferType("3")).toBe(3);
    expect(inferType("-45.5")).toBe(-45.5);
  });

  it("handles special value strings", () => {
    expect(inferType("true")).toBe(true);
    expect(inferType("TRUE")).toBe(true);
    expect(inferType("false")).toBe(false);
    expect(inferType("FALSE")).toBe(false);

    expect(inferType("undefined")).toBe(undefined);
    expect(inferType("UNDEFINED")).toBe(undefined);

    expect(inferType("nan")).toBe("NaN");
    expect(inferType("NaN")).toBe("NaN");
    expect(inferType("NAN")).toBe("NaN");

    expect(inferType("null")).toBe(null);
    expect(inferType("NULL")).toBe(null);

    expect(inferType("inf")).toBe("Infinity");
    expect(inferType("infinity")).toBe("Infinity");
    expect(inferType("-inf")).toBe("-Infinity");
    expect(inferType("-infinity")).toBe("-Infinity");
  });

  it("handles an empty string as undefined", () => {
    expect(inferType("")).toBe(undefined);
  });

  it("handles a string with two double XOR single quotes as an empty string", () => {
    expect(inferType('""')).toBe("");
    expect(inferType("''")).toBe("");
    expect(inferType(`'"`)).toBe(`'"`);
    expect(inferType(`"'`)).toBe(`"'`);
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
    expect(inferType("[a, null, 34]")).toEqual(["a", null, 34]);
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

  it("infers strings that start with a comma as arrays", () => {
    expect(inferType(",")).toEqual([]);
    expect(inferType(",3")).toEqual([3]);
    expect(inferType(",abc")).toEqual(["abc"]);
    expect(inferType(",14,abc,null")).toEqual([14, "abc", null]);
  });

  it("infers strings that end with a comma as arrays", () => {
    expect(inferType(",")).toEqual([]);
    expect(inferType("3,")).toEqual([3]);
    expect(inferType("abc,")).toEqual(["abc"]);
    expect(inferType("14,abc,null,")).toEqual([14, "abc", null]);
  });

  it("turns arrays with undefined in them into null", () => {
    // TODO: maybe get RSJON to turn undefineds into null?
    // expect(inferType("[undefined]")).toEqual([null]);
    // expect(inferType("[undefined, 3]")).toEqual([null, 3]);
    expect(inferType(",abc,undefined,4")).toEqual(["abc", null, 4]);
  });

  it("parses weird looking things as strings", () => {
    expect(inferType("{what: even is this)) ]}")).toEqual(
      "{what: even is this)) ]}",
    );
    expect(inferType("(1,2,3)")).toEqual("(1,2,3)");
    expect(inferType("NAN/null")).toEqual("NAN/null");
  });

  it("parses a \\. as a literal period", () => {
    expect(inferType(String.raw`\.`)).toBe(".");
    expect(inferType(String.raw`\.`, "field", "value")).toBe(".");
    expect(() => inferType(String.raw`\.`, "linkDur", "value")).toThrowError(
      BadDurationError,
    );
  });
});



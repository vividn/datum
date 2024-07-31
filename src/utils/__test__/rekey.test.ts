import { rekey, RekeyError } from "../rekey";

describe("rekey", () => {
  it("renames keys in aData based off of the key value pairs in bData", () => {
    const aData = { a: 1, b: 2, c: 3 };
    const bData = { a: "A", b: "B" };
    const expected = { A: 1, B: 2, c: 3 };
    const actual = rekey(aData, bData);
    expect(actual).toEqual(expected);
  });

  it("ignores keys in bData that are not in aData", () => {
    const aData = { a: 1, b: 2, c: 3 };
    const bData = { a: "A", b: "B", d: "D" };
    const expected = { A: 1, B: 2, c: 3 };
    const actual = rekey(aData, bData);
    expect(actual).toEqual(expected);
  });

  it("can swap two keys", () => {
    const aData = { a: 1, b: 2, c: 3 };
    const bData = { a: "b", b: "a" };
    const expected = { b: 1, a: 2, c: 3 };
    const actual = rekey(aData, bData);
    expect(actual).toEqual(expected);
  });

  it("can rename nested keys", () => {
    const aData = { a: { b: 1, c: 2, e: 5 }, d: 3 };
    const bData = { a: { b: "B", c: "C" } };
    const expected = { a: { B: 1, C: 2, e: 5 }, d: 3 };
    const actual = rekey(aData, bData);
    expect(actual).toEqual(expected);
  });

  it("throws an error if bData has any values that aren't strings", () => {
    const aData = { a: 1, b: 2, c: 3 };
    const bData = { a: "A", b: 2 };
    expect(() => rekey(aData, bData)).toThrow(RekeyError);

    const bData2 = { a: "A", b: null };
    expect(() => rekey(aData, bData2)).toThrow(RekeyError);

    const bData3 = { a: "A", b: undefined };
    expect(() => rekey(aData, bData3)).toThrow(RekeyError);

    const bData4 = { a: "A", b: 3 };
    expect(() => rekey(aData, bData4)).toThrow(RekeyError);

    const bData5 = { a: "A", b: ["a", "b"] };
    expect(() => rekey(aData, bData5)).toThrow(RekeyError);
  });

  it("throws an error for a nested rekey if a isn't also nested", () => {
    const aData = { a: 1, b: 2, c: 3 };
    const bData = { a: { b: "B" } };
    expect(() => rekey(aData, bData)).toThrow(RekeyError);
  });

  it("should clobber existing keys", () => {
    const aData = { a: 1, b: 2, c: 3 };
    const bData = { a: "b" };
    const expected = { b: 1, c: 3 };
    const actual = rekey(aData, bData);
    expect(actual).toEqual(expected);

    const bData2 = { b: "a" };
    const expected2 = { a: 2, c: 3 };
    expect(rekey(aData, bData2)).toEqual(expected2);
  });
});

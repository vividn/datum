import { rekey } from "../rekey";

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
    const aData = { a: { b: 1, c: 2 }, d: 3 };
    const bData = { a: { b: "B" } };
    const expected = { a: { B: 1, c: 2 }, d: 3 };
    const actual = rekey(aData, bData);
    expect(actual).toEqual(expected);
  });
});

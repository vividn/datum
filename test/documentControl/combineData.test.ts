import { describe, expect, test } from "@jest/globals";
import jClone from "../../src/utils/jClone";
import combineData, {
  conflictingKeyStrategies,
  mergeValues,
} from "../../src/documentControl/combineData";
import { MergeError } from "../../src/errors";

describe("combineData", () => {
  const aData = {
    justA: "aaa",
    bothSame: "same",
    different: "Aa",
  };
  const bData = {
    justB: "bbb",
    bothSame: "same",
    different: "bB",
  };

  for (const justAVal of [true, false]) {
    for (const justBVal of [true, false]) {
      for (const sameVal of [true, false]) {
        for (const conflictVal of [
          "A",
          "B",
          "merge",
          "append",
          false,
        ] as conflictingKeyStrategies[]) {
          const testCaseDescription = `(${justAVal},${justBVal},${sameVal},${conflictVal})`;
          const combined = combineData(aData, bData, {
            justA: justAVal,
            justB: justBVal,
            same: sameVal,
            conflict: conflictVal,
          });

          if (justAVal) {
            test(`fields just in A are kept with ${testCaseDescription}`, () => {
              expect(combined).toHaveProperty("justA", "aaa");
            });
          } else {
            test(`fields just in A are excluded with ${testCaseDescription}`, () => {
              expect(combined).not.toHaveProperty("justA");
            });
          }

          if (justBVal) {
            test(`fields just in B are kept with ${testCaseDescription}`, () => {
              expect(combined).toHaveProperty("justB", "bbb");
            });
          } else {
            test(`fields just in B are excluded with ${testCaseDescription}`, () => {
              expect(combined).not.toHaveProperty("justB");
            });
          }

          if (sameVal) {
            test(`fields in both are kept with ${testCaseDescription}`, () => {
              expect(combined).toHaveProperty("bothSame", "same");
            });
          } else {
            test(`fields in both are excluded with ${testCaseDescription}`, () => {
              expect(combined).not.toHaveProperty("bothSame");
            });
          }

          if (conflictVal === "A") {
            test(`conflicting fields use the value in A with ${testCaseDescription}`, () => {
              expect(combined).toHaveProperty("different", "Aa");
            });
          } else if (conflictVal === "B") {
            test(`conflicting fields use the value in B with ${testCaseDescription}`, () => {
              expect(combined).toHaveProperty("different", "bB");
            });
          } else if (conflictVal === "merge" || conflictVal === "append") {
            test(`conflicting fields merge into single array with ${testCaseDescription}`, () => {
              expect(combined).toHaveProperty("different", ["Aa", "bB"]);
            });
          } else {
            test(`conflicting fields excluded with ${testCaseDescription}`, () => {
              expect(combined).not.toHaveProperty("different");
            });
          }
        }
      }
    }
  }

  test("useOld only returns oldData", () => {
    const ret = combineData(aData, bData, "useOld");
    expect(ret).toEqual(aData);
  });

  test("useNew only returns newData", () => {
    const ret = combineData(aData, bData, "useNew");
    expect(ret).toEqual(bData);
  });

  test("preferOld keeps non conflicting keys in new, but prefers the old values", () => {
    const ret = combineData(aData, bData, "preferOld");
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same",
      different: "Aa",
    });
  });

  test("preferNew keeps non-conflicting keys in old, but prefers the new values", () => {
    const ret = combineData(aData, bData, "preferNew");
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same",
      different: "bB",
    });
  });

  test("intersection only keeps keys that are in both and agree", () => {
    const ret = combineData(aData, bData, "intersection");
    expect(ret).toEqual({
      bothSame: "same",
    });
  });

  test("removeConflicting only keeps non-conflicting keys from both", () => {
    const ret = combineData(aData, bData, "removeConflicting");
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same",
    });
  });

  test("xor keeps keys that appear in one or the other but not in both", () => {
    const ret = combineData(aData, bData, "xor");
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
    });
  });

  test("merge combines conflicting fields into arrays removing duplicates", () => {
    const aMerge = { ...aData, someDataDuplicated: ["unique1", "duplicate"] };
    const bMerge = { ...bData, someDataDuplicated: ["unique2", "duplicate"] };
    expect(combineData(aMerge, bMerge, "merge")).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same",
      different: ["Aa", "bB"],
      someDataDuplicated: ["unique1", "duplicate", "unique2"],
    });
  });

  test("append combines conflicting fields into arrays keeping duplicates", () => {
    const aMerge = { ...aData, someDataDuplicated: ["unique1", "duplicate"] };
    const bMerge = { ...bData, someDataDuplicated: ["unique2", "duplicate"] };
    expect(combineData(aMerge, bMerge, "append")).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same",
      different: ["Aa", "bB"],
      someDataDuplicated: ["unique1", "duplicate", "unique2", "duplicate"],
    });
  });

  test("does not mutate or return original data", () => {
    const aClone = jClone(aData);
    const bClone = jClone(bData);
    const ret = combineData(aData, bData, "useOld");
    expect(aData).toEqual(aClone);
    expect(bData).toEqual(bClone);
    ret["newField"] = "newData";
    expect(aData).not.toHaveProperty("newField");

    combineData(aData, bData, "merge");
    expect(aData).toEqual(aClone);
    expect(bData).toEqual(bClone);
  });

  test("it recursively calls update if the key is an Object in both", () => {
    const deepA = {
      outerKey: {
        innerA: "aaa",
        innerSame: "samesame",
        innerDifferent: "fromA",
      },
      onlyObjForA: {
        someKey: "someValue",
      },
    };
    const deepB = {
      outerKey: {
        innerB: "bbb",
        innerSame: "samesame",
        innerDifferent: "fromB",
      },
      onlyObjForA: "bHasAString",
    };

    const combined = combineData(deepA, deepB, "preferNew");
    expect(combined).toEqual({
      outerKey: {
        innerA: "aaa",
        innerB: "bbb",
        innerSame: "samesame",
        innerDifferent: "fromB",
      },
      onlyObjForA: "bHasAString",
    });
  });
});

describe("mergeValues", () => {
  test("if a value is undefined it returns the other value", () => {
    expect(mergeValues("string", undefined)).toEqual("string");
    expect(mergeValues(undefined, 3)).toEqual(3);
    expect(mergeValues(null, undefined)).toEqual(null);
    expect(mergeValues(undefined, ["abc", 123])).toEqual(["abc", 123]);
    expect(mergeValues({ abc: 123 }, undefined)).toEqual({ abc: 123 });
    expect(mergeValues(undefined, undefined)).toEqual(undefined);
  });

  test("combines two singleton strings, numbers, or nulls into an array", () => {
    expect(mergeValues("string1", "string2")).toEqual(["string1", "string2"]);
    expect(mergeValues("string", 3)).toEqual(["string", 3]);
    expect(mergeValues(3, "string")).toEqual([3, "string"]);
    expect(mergeValues(null, "string")).toEqual([null, "string"]);
    expect(mergeValues(5, null)).toEqual([5, null]);
    expect(mergeValues(5, 25)).toEqual([5, 25]);
  });

  test("can append singleton value to array", () => {
    expect(mergeValues(["string1", "string2"], "string3")).toEqual([
      "string1",
      "string2",
      "string3",
    ]);
    expect(mergeValues([3, "string"], null)).toEqual([3, "string", null]);
    expect(mergeValues(["justOneInArray"], 2)).toEqual(["justOneInArray", 2]);
  });

  test("can prepend singleton value to array", () => {
    expect(mergeValues("string1", ["string2", "string3"])).toEqual([
      "string1",
      "string2",
      "string3",
    ]);
    expect(mergeValues(null, [3, "string"])).toEqual([null, 3, "string"]);
    expect(mergeValues(2, ["justOneInArray"])).toEqual([2, "justOneInArray"]);
  });

  test("appends two arrays", () => {
    expect(mergeValues(["string", 3, null], ["string2", 7])).toEqual([
      "string",
      3,
      null,
      "string2",
      7,
    ]);
  });

  test("removes duplicate values from array if doing unique merge", () => {
    expect(
      mergeValues(
        [1, "uniqueString", "duplicatedString", 2],
        [2, "anotherUnique", "duplicatedString", 5],
        true
      )
    ).toEqual([1, "uniqueString", "duplicatedString", 2, "anotherUnique", 5]);
  });

  test("keeps duplicate values if not doing unique merge", () => {
    expect(
      mergeValues(
        [1, "uniqueString", "duplicatedString", 2],
        [2, "anotherUnique", "duplicatedString", 5],
        false
      )
    ).toEqual([
      1,
      "uniqueString",
      "duplicatedString",
      2,
      2,
      "anotherUnique",
      "duplicatedString",
      5,
    ]);
  });

  test("throws error if trying to merge an object with anything (except undefined)", () => {
    expect(() => mergeValues({ abc: 123 }, "string")).toThrowError(MergeError);
    expect(() => mergeValues(3, { abc: 123 })).toThrowError(MergeError);
    expect(() => mergeValues({ someKey: "someData" }, null)).toThrowError(
      MergeError
    );
    expect(() => mergeValues([1, 2, 3], { qwerty: "asdf" })).toThrowError(
      MergeError
    );
    expect(() =>
      mergeValues({ even: "two" }, { objects: "fail" })
    ).toThrowError(MergeError);
  });
});

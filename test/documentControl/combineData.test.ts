import { describe, expect, test } from "@jest/globals";
import {
  combineData,
  conflictStrategies,
} from "../../src/documentControl/updateDoc";
import jClone from "../../src/utils/jClone";

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
          false,
        ] as conflictStrategies[]) {
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
          } else if (conflictVal === "merge") {
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

  test("preferNew keeps non-conflicting keys in old, but perfers the new values", () => {
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
        innerDifferent: "fromA"
      },
      onlyObjForA: {
        someKey: "someValue"
      }
    };
    const deepB = {
      outerKey: {
        innerB: "bbb",
        innerSame: "samesame",
        innerDifferent: "fromB"
      },
      onlyObjForA: "bHasAString"
    };

    const combined = combineData(deepA, deepB, "preferNew");
    expect(combined).toEqual({
      outerKey: {
        innerA: "aaa",
        innerB: "bbb",
        innerSame: "samesame",
        innerDifferent: "fromB"
      },
      onlyObjForA: "bHasAString"
    });
  })
});

describe("merge", () => {
  test.todo(
    "fields that appear in one but not in the other appear with their value in the output"
  );
});

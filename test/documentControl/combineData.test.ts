import { describe, expect, test } from "@jest/globals";
import { combineData, conflictStrategies } from "../../src/documentControl/updateDoc";

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
        for (const conflictVal of ["A", "B", "merge", false] as conflictStrategies[]) {
          const testCaseDescription = `(${justAVal},${justBVal},${sameVal},${conflictVal})`;
          const combined = combineData(aData, bData, {justA: justAVal, justB: justBVal, same: sameVal, conflict: conflictVal});

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

  test.todo("useOld only returns oldData", () => {
    const ret = useOld(aData, bData);
    expect(ret).toEqual(aData);
  });

  test.todo("useNew only returns newData", () => {
    const ret = useNew(aData, bData);
    expect(ret).toEqual(bData);
  });

  test.todo("preferOld keeps non conflicting keys in new, but prefers the old values", () => {
    const ret = preferOld(aData, bData);
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same",
      different: "Aa",
    });
  });

  test.todo("preferNew keeps non-conflicting keys in old, but perfers the new values", () => {
    const ret = preferNew(aData, bData);
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same",
      different: "bB"
    });
  });

  test.todo("intersection only keeps keys that are in both and agree", () => {
    const ret = intersection(aData, bData);
    expect(ret).toEqual({
      bothSame: "same"
    });
  });

  test.todo("removeConflicting only keeps non-conflicting keys from both", () => {
    const ret = removeConflicting(aData, bData);
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
      bothSame: "same"
    });
  });

  test.todo("xor keeps keys that appear in one or the other but not in both", () => {
    const ret = xor(aData, bData);
    expect(ret).toEqual({
      justA: "aaa",
      justB: "bbb",
    });
  });

  test.todo("all update functions do not mutate or return original data");


  describe ("merge", () => {
    test.todo("fields that appear in one but not in the other appear with their value in the output");
  });
});
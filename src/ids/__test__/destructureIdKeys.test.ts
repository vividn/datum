import { describe, expect, it } from "@jest/globals";
import { destructureIdKeys } from "../destructureIdKeys";

describe("destructureIdKeys", () => {
  const obj = { a: 1, b: 2, c: 3 };
  it("can pull out individual keys", () => {
    expect(destructureIdKeys(obj, "%b%")).toMatchObject({
      onlyFields: { b: 2 },
      noFields: { a: 1, c: 3 },
    });
  });

  it("can pull out multiple keys", () => {
    expect(destructureIdKeys(obj, "%b%__%c%")).toMatchObject({
      onlyFields: { b: 2, c: 3 },
      noFields: { a: 1 },
    });
  });

  it("does not treat raw strings as keys", () => {
    expect(destructureIdKeys(obj, "a")).toMatchObject({
      onlyFields: {},
      noFields: { a: 1, b: 2, c: 3 },
    });
    expect(destructureIdKeys(obj, "c%a%b")).toMatchObject({
      onlyFields: { a: 1 },
      noFields: { b: 2, c: 3 },
    });
  });

  it("shows missing keys as undefined", () => {
    expect(destructureIdKeys(obj, "%notAKey%")).toMatchObject({
      onlyFields: { notAKey: undefined },
      noFields: obj,
    });
  });

  it("handles nested objects", () => {
    const nestedObj = { a: { nested1: "one", nested2: "two" }, b: 2 };

    expect(destructureIdKeys(nestedObj, "%a%")).toMatchObject({
      onlyFields: { a: { nested1: "one", nested2: "two" } },
      noFields: { b: 2 },
    });
    expect(destructureIdKeys(nestedObj, "%a.nested1%")).toMatchObject({
      onlyFields: { a: { nested1: "one" } },
      noFields: { a: { nested2: "two" }, b: 2 },
    });
    expect(
      destructureIdKeys(nestedObj, "%a.nested1%__%a.nested2%")
    ).toMatchObject({
      onlyFields: { a: { nested1: "one", nested2: "two" } },
      noFields: { a: {}, b: 2 },
    });
  });

  it("can use meta.idStructure to grab keys if idStructure not explicit", () => {
    const objWithMeta = {
      a: { b: 2, bb: 55 },
      c: 3,
      meta: { idStructure: "%a.bb%%c%" },
    };
    expect(destructureIdKeys(objWithMeta)).toMatchObject({
      onlyFields: { a: { bb: 55 }, c: 3 },
      noFields: { a: { b: 2 }, meta: { idStructure: "%a.bb%%c%" } },
    });
  });
});

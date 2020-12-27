const { assembleId, destructureIdKeys } = require("../src/ids");

const testPayload = {
  meta: {
    occurTime: "2020-11-09T00:35:10.000Z",
    createTime: "2020-11-09T00:40:12.544Z",
    modifyTime: "2020-11-09T00:40:12.544Z",
    utcOffset: 1,
  },
  foo: "abc",
  bar: "def",
  complex: { data: "nested" },
  array: ["various", 2, "data"],
  num: 3,
  "wei%rd": "da%ta",
};

const testPayloadWithPartition = { ...testPayload, field: "main" };

const expectAssembleIdReturns = (props, expectedReturn) => {
  expect(
    assembleId({ payload: testPayload, ...props }),
    JSON.stringify(props)
  ).toMatchObject(expectedReturn);
};

describe("assembleId", () => {
  it("returns the occurTime as a default id", () => {
    expectAssembleIdReturns(
      {},
      { id: testPayload.meta.occurTime, structure: "%meta.occurTime%" }
    );
  });

  it("can assemble single component ids", () => {
    expectAssembleIdReturns(
      { idPart: "%foo%" },
      { id: "abc", structure: "%foo%" }
    );
    expectAssembleIdReturns(
      { idPart: "%bar%" },
      { id: "def", structure: "%bar%" }
    );
  });

  it("can use raw strings", () => {
    expectAssembleIdReturns({ idPart: "foo" }, { id: "foo", structure: "foo" });
    expectAssembleIdReturns(
      { idPart: "manual_raw$id" },
      { id: "manual_raw$id", structure: "manual_raw$id" }
    );
  });

  it("can interpolate raw strings and field names", () => {
    expectAssembleIdReturns(
      { idPart: "foo%foo%" },
      { id: "fooabc", structure: "foo%foo%" }
    );
    expectAssembleIdReturns(
      { idPart: "%foo%_:)_%bar%" },
      { id: "abc_:)_def", structure: "%foo%_:)_%bar%" }
    );
    expectAssembleIdReturns(
      { idPart: "raw%foo" },
      { id: "rawabc", structure: "raw%foo%" }
    );
  });

  it("automatically infers a missing trailing %", () => {
    expectAssembleIdReturns(
      { idPart: "%foo" },
      { id: "abc", structure: "%foo%" }
    );
    expectAssembleIdReturns(
      { idPart: "raw%foo" },
      { id: "rawabc", structure: "raw%foo%" }
    );
    expectAssembleIdReturns(
      { idPart: ["%foo", "%bar"] },
      { id: "abc__def", structure: "%foo%__%bar%" }
    );
  });

  it("combines multiple components with the id_delimiter", () => {
    expectAssembleIdReturns(
      { idPart: ["%foo", "%bar"] },
      { id: "abc__def", structure: "%foo%__%bar%" }
    );
    expectAssembleIdReturns(
      { idPart: ["%foo%", "raw", "%bar"] },
      { id: "abc__raw__def", structure: "%foo%__raw__%bar%" }
    );
    expectAssembleIdReturns(
      { idPart: ["%foo", "%bar"], delimiter: "@" },
      { id: "abc@def", structure: "%foo%@%bar%" }
    );
    expectAssembleIdReturns(
      { idPart: ["%foo", "%bar"], delimiter: "" },
      { id: "abcdef", structure: "%foo%%bar%" }
    );
    expectAssembleIdReturns(
      { idPart: ["%foo", "%bar", "rawString"], delimiter: "%" },
      { id: "abc%def%rawString", structure: "%foo%\\%%bar%\\%rawString" }
    );
  });

  it("omits fields that do not exist", () => {
    expectAssembleIdReturns(
      { idPart: "%notAField" },
      { id: "", structure: "%notAField%" }
    );
    expectAssembleIdReturns(
      { idPart: ["%foo", "%notAField", "%bar"] },
      { id: "abc____def", structure: "%foo%__%notAField%__%bar%" }
    );
  });

  it("can retrieve deeper values", () => {
    expectAssembleIdReturns(
      { idPart: "%meta.createTime" },
      { id: testPayload.meta.createTime, structure: "%meta.createTime%" }
    );
    expectAssembleIdReturns(
      { idPart: "%complex.data" },
      { id: "nested", structure: "%complex.data%" }
    );
    expectAssembleIdReturns(
      { idPart: ["%foo", "%complex.notAKey"] },
      { id: "abc__", structure: "%foo%__%complex.notAKey%" }
    );
    expectAssembleIdReturns(
      { idPart: "%not.real.keys" },
      { id: "", structure: "%not.real.keys%" }
    );
  });

  it("serializes numbers, objects, and arrays", () => {
    expectAssembleIdReturns({ idPart: "%num" }, { id: "3" });
    expectAssembleIdReturns(
      { idPart: "%complex" },
      { id: '{"data":"nested"}' }
    );
    expectAssembleIdReturns(
      { idPart: "%array" },
      { id: '["various",2,"data"]' }
    );
  });

  it("escapes percent signs properly", () => {
    expectAssembleIdReturns(
      { idPart: "raw with escaped \\%foo" },
      { id: "raw with escaped %foo", structure: "raw with escaped \\%foo" }
    );
    expectAssembleIdReturns(
      { idPart: "%wei\\%rd" },
      { id: "da%ta", structure: "%wei\\%rd%" }
    );
  });

  it("prepends the partition field if provided", () => {
    expectAssembleIdReturns(
      { payload: testPayloadWithPartition },
      {
        id: "main:" + testPayload.meta.occurTime,
        structure: "%field%:%meta.occurTime%",
      }
    );
    expectAssembleIdReturns(
      { idPart: "%foo", payload: { ...testPayload, field: "otherName" } },
      { id: "otherName:abc", structure: "%field%:%foo%" }
    );
    expectAssembleIdReturns(
      { payload: { field: "onlyField" } },
      { id: "onlyField:", structure: "%field%:%meta.occurTime%" }
    );
  });

  it("can use other fields, strings, and combinations as partition", () => {
    expectAssembleIdReturns(
      { partition: "%foo", idPart: "%bar" },
      { id: "abc:def", structure: "%foo%:%bar%" }
    );
    expectAssembleIdReturns(
      { partition: "%bar", idPart: "%bar" },
      { id: "def:def", structure: "%bar%:%bar%" }
    );
    expectAssembleIdReturns(
      { partition: "rawString", idPart: ["%foo", "raw"] },
      { id: "rawString:abc__raw", structure: "rawString:%foo%__raw" }
    );
    expectAssembleIdReturns(
      { partition: ["%foo", "%bar%-with-extra"], idPart: "id" },
      { id: "abc__def-with-extra:id", structure: "%foo%__%bar%-with-extra:id" }
    );
    expectAssembleIdReturns(
      {
        partition: ["%foo", "%bar"],
        idPart: ["some", "strings"],
        delimiter: "!",
      },
      { id: "abc!def:some!strings", structure: "%foo%!%bar%:some!strings" }
    );
  });

  it("handles this example", () => {
    expectAssembleIdReturns(
      {
        idPart: ["%foo", "%meta.occurTime", "rawString"],
        delimiter: "__",
        partition: "%field",
        payload: testPayloadWithPartition,
      },
      {
        id: "main:abc__2020-11-09T00:35:10.000Z__rawString",
        structure: "%field%:%foo%__%meta.occurTime%__rawString",
      }
    );
  });

  it("prefers to use the _id in the payload", () => {
    expectAssembleIdReturns(
      {
        idPart: "%foo",
        payload: { foo: "fooId", _id: "_idId" },
      },
      { id: "_idId" }
    );
  });
});

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

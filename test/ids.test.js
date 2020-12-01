const { assembleId } = require("../src/ids");

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
      { idField: "%foo%" },
      { id: "abc", structure: "%foo%" }
    );
    expectAssembleIdReturns(
      { idField: "%bar%" },
      { id: "def", structure: "%bar%" }
    );
  });

  it("can use raw strings", () => {
    expectAssembleIdReturns(
      { idField: "foo" },
      { id: "foo", structure: "foo" }
    );
    expectAssembleIdReturns(
      { idField: "manual_raw$id" },
      { id: "manual_raw$id", structure: "manual_raw$id" }
    );
  });

  it("can interpolate raw strings and field names", () => {
    expectAssembleIdReturns(
      { idField: "foo%foo%" },
      { id: "fooabc", structure: "foo%foo%" }
    );
    expectAssembleIdReturns(
      { idField: "%foo%_:)_%bar%" },
      { id: "abc_:)_def", structure: "%foo%_:)_%bar%" }
    );
    expectAssembleIdReturns(
      { idField: "raw%foo" },
      { id: "rawabc", structure: "raw%foo%" }
    );
  });

  it("automatically infers a missing trailing %", () => {
    expectAssembleIdReturns(
      { idField: "%foo" },
      { id: "abc", structure: "%foo%" }
    );
    expectAssembleIdReturns(
      { idField: "raw%foo" },
      { id: "rawabc", structure: "raw%foo%" }
    );
    expectAssembleIdReturns(
      { idField: ["%foo", "%bar"] },
      { id: "abc__def", structure: "%foo%__%bar%" }
    );
  });

  it("combines multiple components with the id_delimiter", () => {
    expectAssembleIdReturns(
      { idField: ["%foo", "%bar"] },
      { id: "abc__def", structure: "%foo%__%bar%" }
    );
    expectAssembleIdReturns(
      { idField: ["%foo%", "raw", "%bar"] },
      { id: "abc__raw__def", structure: "%foo%__raw__%bar%" }
    );
    expectAssembleIdReturns(
      { idField: ["%foo", "%bar"], delimiter: "@" },
      { id: "abc@def", structure: "%foo%@%bar%" }
    );
    expectAssembleIdReturns(
      { idField: ["%foo", "%bar"], delimiter: "" },
      { id: "abcdef", structure: "%foo%%bar%" }
    );
    expectAssembleIdReturns(
      { idField: ["%foo", "%bar", "rawString"], delimiter: "%" },
      { id: "abc%def%rawString", structure: "%foo%\\%%bar%\\%rawString" }
    );
  });

  it("omits fields that do not exist", () => {
    expectAssembleIdReturns(
      { idField: "%notAField" },
      { id: "", structure: "%notAField%" }
    );
    expectAssembleIdReturns(
      { idField: ["%foo", "%notAField", "%bar"] },
      { id: "abc____def", structure: "%foo%__%notAField%__%bar%" }
    );
  });

  it("can retrieve deeper values", () => {
    expectAssembleIdReturns(
      { idField: "%meta.createTime" },
      { id: testPayload.meta.createTime, structure: "%meta.createTime%" }
    );
    expectAssembleIdReturns(
      { idField: "%complex.data" },
      { id: "nested", structure: "%complex.data%" }
    );
    expectAssembleIdReturns(
      { idField: ["%foo", "%complex.notAKey"] },
      { id: "abc__", structure: "%foo%__%complex.notAKey%" }
    );
    expectAssembleIdReturns(
      { idField: "%not.real.keys" },
      { id: "", structure: "%not.real.keys%" }
    );
  });

  it("serializes numbers, objects, and arrays", () => {
    expectAssembleIdReturns({ idField: "%num" }, { id: "3" });
    expectAssembleIdReturns(
      { idField: "%complex" },
      { id: '{"data":"nested"}' }
    );
    expectAssembleIdReturns(
      { idField: "%array" },
      { id: '["various",2,"data"]' }
    );
  });

  it("escapes percent signs properly", () => {
    expectAssembleIdReturns(
      { idField: "raw with escaped \\%foo" },
      { id: "raw with escaped %foo", structure: "raw with escaped \\%foo" }
    );
    expectAssembleIdReturns(
      { idField: "%wei\\%rd" },
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
      { idField: "%foo", payload: { ...testPayload, field: "otherName" } },
      { id: "otherName:abc", structure: "%field%:%foo%" }
    );
    expectAssembleIdReturns(
      { payload: { field: "onlyField" } },
      { id: "onlyField:", structure: "%field%:%meta.occurTime%" }
    );
  });

  it("can use other fields, strings, and combinations as partition", () => {
    expectAssembleIdReturns(
      { partitionField: "%foo", idField: "%bar" },
      { id: "abc:def", structure: "%foo%:%bar%" }
    );
    expectAssembleIdReturns(
      { partitionField: "%bar", idField: "%bar" },
      { id: "def:def", structure: "%bar%:%bar%" }
    );
    expectAssembleIdReturns(
      { partitionField: "rawString", idField: ["%foo", "raw"] },
      { id: "rawString:abc__raw", structure: "rawString:%foo%__raw" }
    );
    expectAssembleIdReturns(
      { partitionField: ["%foo", "%bar%-with-extra"], idField: "id" },
      { id: "abc__def-with-extra:id", structure: "%foo%__%bar%-with-extra:id" }
    );
    expectAssembleIdReturns(
      {
        partitionField: ["%foo", "%bar"],
        idField: ["some", "strings"],
        delimiter: "!",
      },
      { id: "abc!def:some!strings", structure: "%foo%!%bar%:some!strings" }
    );
  });

  it("handles this example", () => {
    expectAssembleIdReturns(
      {
        idField: ["%foo", "%meta.occurTime", "rawString"],
        delimiter: "__",
        partitionField: "%field",
        payload: testPayloadWithPartition,
      },
      {
        id: "main:abc__2020-11-09T00:35:10.000Z__rawString",
        structure: "%field%:%foo%__%meta.occurTime%__rawString",
      }
    );
  });
});

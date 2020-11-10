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
  complex: { data: "structure" },
  array: ["various", 2, "data"],
  num: 3,
  "wei'rd": "da'ta",
};

const testPayloadWithPartition = { ...testPayload, field: "main" };

const expectAssembleIdReturns = (props, expectedReturn) => {
  expect(
    assembleId({ payload: testPayload, ...props }),
    JSON.stringify(props)
  ).toBe(expectedReturn);
};

describe("assembleId", () => {
  it("returns the occurTime as a default id", () => {
    expectAssembleIdReturns({}, testPayload.meta.occurTime);
  });

  it("can assemble single component ids", () => {
    expectAssembleIdReturns({ idField: "foo" }, "abc");
    expectAssembleIdReturns({ idField: "bar" }, "def");
  });

  it("interprets single quotes as raw strings", () => {
    expectAssembleIdReturns({ idField: "'foo'" }, "foo");
    expectAssembleIdReturns({ idField: "'manual_raw$id'" }, "manual_raw$id");
    expectAssembleIdReturns(
      { idField: "'missing_ending_quote_ignored" },
      "missing_ending_quote_ignored"
    );
  });

  it("can interpolate raw strings and field names", () => {
    expectAssembleIdReturns({ idField: "'foo'foo" }, "fooabc");
    expectAssembleIdReturns({ idField: "foo'_:)_'bar" }, "abc_:)_def");
    expectAssembleIdReturns({ idField: "foo'raw" }, "abcraw");
    expectAssembleIdReturns({ idField: "foo''bar" }, "abcdef");
  });

  it("escapes single quotes properly", () => {
    expectAssembleIdReturns(
      { idField: "'raw with \\'escaped quotes\\''" },
      "raw with 'escaped quotes'"
    );
    expectAssembleIdReturns({ idField: "wei\\'rd" }, "da'ta");
  });

  it("combines multiple components with the id_delimiter", () => {
    expectAssembleIdReturns({ idField: ["foo", "bar"] }, "abc__def");
    expectAssembleIdReturns(
      { idField: ["foo", "bar"], idDelimiter: "@" },
      "abc@def"
    );
    expectAssembleIdReturns(
      { idField: ["foo", "bar"], idDelimiter: "" },
      "abcdef"
    );
    expectAssembleIdReturns({ idField: ["foo", "'raw'"] }, "abc__raw");
  });

  it("omits fields that do not exist", () => {
    expectAssembleIdReturns({ idField: "notAField" }, "");
    expectAssembleIdReturns(
      { idField: ["foo", "notAField", "bar"] },
      "abc____def"
    );
  });

  it("can retrieve deeper values", () => {
    expectAssembleIdReturns(
      { idField: "meta.createTime" },
      testPayload.meta.createTime
    );
    expectAssembleIdReturns({ idField: "complex.data" }, "structure");
    expectAssembleIdReturns({ idField: ["foo", "complex.notAKey"]}, "abc__")
    expectAssembleIdReturns({ idField: "not.real.keys"}, "")
  });

  it("serializes numbers, objects, and arrays", () => {
    expectAssembleIdReturns({ idField: "num" }, "3");
    expectAssembleIdReturns({ idField: "complex" }, '{"data":"structure"}');
    expectAssembleIdReturns({ idField: "array"}, '["various",2,"data"]')
  });

  it("prepends the partition field if provided", () => {
    expectAssembleIdReturns(
      { payload: testPayloadWithPartition },
      "main:" + testPayload.meta.occurTime
    );
    expectAssembleIdReturns(
      { idField: "foo", payload: { ...testPayload, field: "otherName" } },
      "otherName:abc"
    );
    expectAssembleIdReturns({ payload: { field: "onlyField" } }, "onlyField:");
  });

  it("can use other fields, strings, and combinations as partition", () => {
    expectAssembleIdReturns(
      { partitionField: "foo", idField: "bar" },
      "abc:def"
    );
    expectAssembleIdReturns(
      { partitionField: "bar", idField: "bar" },
      "def:def"
    );
    expectAssembleIdReturns(
      { partitionField: "'rawString'", idField: ["foo", "'raw'"] },
      "rawString:abc__raw"
    );
    expectAssembleIdReturns(
      { partitionField: ["foo", "bar'-with-extra'"], idField: "'id'" },
      "abc__def-with-extra:id"
    );
    expectAssembleIdReturns(
      {
        partitionField: ["foo", "bar"],
        idField: ["'some'", "'strings'"],
        idDelimiter: "!",
      },
      "abc!def:some!strings"
    );
  });

  it("handles this example", () => {
    expectAssembleIdReturns(
      {
        idComponents: ["foo", "occurTime", "'rawString'"],
        idDelimiter: "__",
        partitionField: "field",
      },
      "main:abc__2020-11-09T00:35:10.000Z__rawString"
    );
  });
});

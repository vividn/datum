import { describe, expect, it } from "@jest/globals";
import { DatumData, DatumMetadata } from "../src/documentControl/DatumDocument";

import {
  assembleId,
  buildIdStructure,
  buildIdStructureType,
  defaultIdComponents,
  destructureIdKeys,
} from "../src/ids";
import { IdError } from "../src/errors";

const exampleData: DatumData = {
  foo: "abc",
  bar: "def",
  complex: { data: "nested" },
  array: ["various", 2, "data"],
  num: 3,
  "wei%rd": "da%ta",
};
const exampleMetaNoOccur: DatumMetadata = {
  createTime: "2020-11-09T00:40:12.544Z",
  modifyTime: "2020-11-09T00:40:12.544Z",
  utcOffset: 1,
  random: 0.7368733800261729,
  humanId: "mqp4znq4cvp3qnj74fgi9",
};
const exampleOccurTime = "2020-11-09T00:35:10.000Z";
const exampleMeta: DatumMetadata = {
  occurTime: exampleOccurTime,
  ...exampleMetaNoOccur,
};

const exampleDataWithField: DatumData = { ...exampleData, field: "main" };

const expectStructureAndId = (
  props: Partial<buildIdStructureType>,
  expectedStructure: string,
  expectedIdOnTestData: string,
  testData: DatumData = exampleData,
  testMeta: DatumMetadata | false = exampleMeta
) => {
  const hasOccurTime = testMeta && "occurTime" in testMeta;
  const { defaultPartitionParts, defaultIdParts } = defaultIdComponents({
    data: testData,
    hasOccurTime,
  });

  const buildIdStructureProps: buildIdStructureType = {
    idParts: props.idParts ?? defaultIdParts,
    partition: props.partition ?? defaultPartitionParts,
    delimiter: props.delimiter,
  };
  const idStructure = buildIdStructure(buildIdStructureProps);
  expect(idStructure).toStrictEqual(expectedStructure);

  const id = assembleId({
    data: testData,
    meta: testMeta || undefined,
    idStructure: idStructure,
  });
  expect(id).toStrictEqual(expectedIdOnTestData);
};

describe("id flow", () => {
  it("returns the occurTime as a default id, if occurTime is in meta", () => {
    expectStructureAndId(
      {},
      "%?occurTime%",
      exampleOccurTime,
      exampleData,
      exampleMeta
    );
  });

  it("uses a concatenation of all fields as the default if no occurTime is present", () => {
    const simpleData: DatumData = { a: 123, b: "abc", three: "value" };
    expectStructureAndId(
      {},
      "%a%__%b%__%three%",
      "123__abc__value",
      simpleData,
      exampleMetaNoOccur
    );

    expectStructureAndId(
      {},
      String.raw`%foo%__%bar%__%complex%__%array%__%num%__%wei\%rd%`,
      String.raw`abc__def__{"data":"nested"}__["various",2,"data"]__3__da%ta`,
      exampleData,
      exampleMetaNoOccur
    );
  });

  it("can assemble single component ids", () => {
    expectStructureAndId({ idParts: "%foo%" }, "%foo%", "abc");
    expectStructureAndId({ idParts: "%bar%" }, "%bar%", "def");
  });

  it("can use raw strings", () => {
    expectStructureAndId({ idParts: "foo" }, "foo", "foo");
    expectStructureAndId(
      { idParts: "manual_raw$id" },
      "manual_raw$id",
      "manual_raw$id"
    );
  });

  it("can interpolate raw strings and field names", () => {
    expectStructureAndId({ idParts: "foo%foo%" }, "foo%foo%", "fooabc");
    expectStructureAndId(
      { idParts: "%foo%_:)_%bar%" },
      "%foo%_:)_%bar%",
      "abc_:)_def"
    );
    expectStructureAndId({ idParts: "raw%foo" }, "raw%foo%", "rawabc");
  });

  it("automatically infers a missing trailing %", () => {
    expectStructureAndId({ idParts: "%foo" }, "%foo%", "abc");
    expectStructureAndId({ idParts: "raw%foo" }, "raw%foo%", "rawabc");
    expectStructureAndId(
      { idParts: ["%foo", "%bar"] },
      "%foo%__%bar%",
      "abc__def"
    );
  });

  it("combines multiple components with the id_delimiter", () => {
    expectStructureAndId(
      { idParts: ["%foo", "%bar"] },
      "%foo%__%bar%",
      "abc__def"
    );
    expectStructureAndId(
      { idParts: ["%foo%", "raw", "%bar"] },
      "%foo%__raw__%bar%",
      "abc__raw__def"
    );
    expectStructureAndId(
      { idParts: ["%foo", "%bar"], delimiter: "@" },
      "%foo%@%bar%",
      "abc@def"
    );
    expectStructureAndId(
      { idParts: ["%foo", "%bar"], delimiter: "" },
      "%foo%%bar%",
      "abcdef"
    );
    expectStructureAndId(
      { idParts: ["%foo", "%bar", "rawString"], delimiter: "%" },
      "%foo%\\%%bar%\\%rawString",
      "abc%def%rawString"
    );
  });

  it("omits fields that do not exist", () => {
    expectStructureAndId({ idParts: "%notAField" }, "%notAField%", "");
    expectStructureAndId(
      { idParts: ["%foo", "%notAField", "%bar"] },
      "%foo%__%notAField%__%bar%",
      "abc____def"
    );
  });

  it("can retrieve deeper values", () => {
    expectStructureAndId(
      { idParts: "%complex.data" },
      "%complex.data%",
      "nested"
    );
    expectStructureAndId(
      { idParts: ["%foo", "%complex.notAKey"] },
      "%foo%__%complex.notAKey%",
      "abc__"
    );
    expectStructureAndId({ idParts: "%not.real.keys" }, "%not.real.keys%", "");
  });

  it("serializes numbers, objects, and arrays", () => {
    expectStructureAndId({ idParts: "%num" }, "%num%", "3");
    expectStructureAndId(
      { idParts: "%complex" },
      "%complex%",
      '{"data":"nested"}'
    );
    expectStructureAndId(
      { idParts: "%array" },
      "%array%",
      '["various",2,"data"]'
    );
  });

  it("escapes percent signs properly", () => {
    expectStructureAndId(
      { idParts: "raw with escaped \\%foo" },
      "raw with escaped \\%foo",
      "raw with escaped %foo"
    );
    expectStructureAndId({ idParts: "%wei\\%rd" }, "%wei\\%rd%", "da%ta");
  });

  it("prepends the partition field if provided", () => {
    expectStructureAndId(
      {},
      "%field%:%?occurTime%",
      "main:" + exampleOccurTime,
      exampleDataWithField
    );
    expectStructureAndId(
      { idParts: "%foo" },
      "%field%:%foo%",
      "otherName:abc",
      { ...exampleData, field: "otherName" }
    );
    expectStructureAndId(
      {},
      "%field%:%field%",
      "onlyField:onlyField",
      {
        field: "onlyField",
      },
      false
    );
  });

  it("can use other fields, strings, and combinations as partition", () => {
    expectStructureAndId(
      { partition: "%foo", idParts: "%bar" },
      "%foo%:%bar%",
      "abc:def"
    );
    expectStructureAndId(
      { partition: "%bar", idParts: "%bar" },
      "%bar%:%bar%",
      "def:def"
    );
    expectStructureAndId(
      { partition: "rawString", idParts: ["%foo", "raw"] },
      "rawString:%foo%__raw",
      "rawString:abc__raw"
    );
    expectStructureAndId(
      { partition: ["%foo", "%bar%-with-extra"], idParts: "id" },
      "%foo%__%bar%-with-extra:id",
      "abc__def-with-extra:id"
    );
    expectStructureAndId(
      {
        partition: ["%foo", "%bar"],
        idParts: ["some", "strings"],
        delimiter: "!",
      },
      "%foo%!%bar%:some!strings",
      "abc!def:some!strings"
    );
  });

  it("handles this example", () => {
    expectStructureAndId(
      {
        idParts: ["%foo", "%?occurTime", "rawString"],
        delimiter: "__",
        partition: "%field",
      },
      "%field%:%foo%__%?occurTime%__rawString",
      "main:abc__2020-11-09T00:35:10.000Z__rawString",
      exampleDataWithField
    );
  });

  it("can use metadata fields by using a question mark before the field", () => {
    expectStructureAndId(
      {
        idParts: "%?occurTime",
      },
      "%?occurTime%",
      "2020-11-09T00:35:10.000Z"
    );
    expectStructureAndId(
      { idParts: ["%?occurTime", "%foo", "%?humanId"] },
      "%?occurTime%__%foo%__%?humanId%",
      "2020-11-09T00:35:10.000Z__abc__mqp4znq4cvp3qnj74fgi9"
    );
  });

  it("can use a dataField that starts with a question mark by escaping the question", () => {
    expectStructureAndId(
      { idParts: "%\\?occurTime%" },
      "%\\?occurTime%",
      "now",
      {
        "?occurTime": "now",
      }
    );
  });

  it("can use metadata fields in the partition", () => {
    expectStructureAndId(
      {
        idParts: "%foo",
        partition: "%?humanId",
      },
      "%?humanId%:%foo%",
      "mqp4znq4cvp3qnj74fgi9:abc"
    );
  });

  it("prefers to use the _id in the data, if in no-metadata mode", () => {
    expectStructureAndId(
      { idParts: "%key" },
      "%key%",
      "id-from-data",
      { key: "id-from-idParts", _id: "id-from-data" },
      false
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

describe("defaultIdComponents", () => {
  it("can use occurTime", () => {
    expect(
      defaultIdComponents({ data: exampleData, hasOccurTime: true })
    ).toMatchObject({ defaultIdParts: ["%?occurTime%"] });
  });

  it("uses a concatenation of data fields if hasOccurTime is false", () => {
    const simpleData = { firstKey: "firstData", secondKey: "secondData" };
    expect(
      defaultIdComponents({ data: simpleData, hasOccurTime: false })
    ).toMatchObject({ defaultIdParts: ["%firstKey%", "%secondKey%"] });
  });

  it("uses field as the default partition", () => {
    expect(
      defaultIdComponents({ data: { field: "abc" }, hasOccurTime: true })
    ).toMatchObject({
      defaultPartitionParts: ["%field%"],
    });
    expect(
      defaultIdComponents({ data: { field: "abc" }, hasOccurTime: false })
    ).toMatchObject({
      defaultPartitionParts: ["%field%"],
    });
    expect(
      defaultIdComponents({
        data: { field: "works", with: "other", keys: "too" },
        hasOccurTime: true,
      })
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
    expect(
      defaultIdComponents({
        data: { field: "works", with: "other", keys: "too" },
        hasOccurTime: false,
      })
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
  });

  it("returns undefined for defaultPartitionParts when no field is present", () => {
    expect(
      defaultIdComponents({
        data: { no: "field", key: "present" },
        hasOccurTime: true,
      })
    ).toMatchObject({ defaultPartitionParts: undefined });
    expect(
      defaultIdComponents({
        data: { no: "field", key: "present" },
        hasOccurTime: false,
      })
    ).toMatchObject({ defaultPartitionParts: undefined });
  });
});

describe("assembleId", () => {
  it("throws error if it cannot find an idStructure in the metadata and idStructure is not provided", () => {
    expect(() =>
      assembleId({
        data: { abc: "123" },
        meta: { occurTime: "2020-11-09T00:40:12.544Z" },
      })
    ).toThrowError(IdError);
    expect(() => assembleId({ data: { abc: "123" } })).toThrowError(IdError);
  });
});

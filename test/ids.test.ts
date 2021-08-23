import { describe, expect, it, test } from "@jest/globals";
import {
  DatumData,
  DatumMetadata,
  EitherPayload,
} from "../src/documentControl/DatumDocument";

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
const exampleOccurTime = "2020-11-09T00:35:10.000Z";
const exampleDataOccur = { occurTime: exampleOccurTime, ...exampleData };
const exampleDataOccurField: DatumData = { ...exampleDataOccur, field: "main" };
const exampleMeta: DatumMetadata = {
  createTime: "2020-11-09T00:40:12.544Z",
  modifyTime: "2020-11-09T00:40:12.544Z",
  utcOffset: 1,
  random: 0.7368733800261729,
  humanId: "mqp4znq4cvp3qnj74fgi9",
};

const expectStructureAndId = (
  props: Partial<buildIdStructureType>,
  expectedStructure: string,
  expectedIdOnTestData: string,
  testData: DatumData = exampleData,
  testMeta: DatumMetadata | false = exampleMeta
) => {
  const { defaultPartitionParts, defaultIdParts } = defaultIdComponents({
    data: testData,
  });
  const payload: EitherPayload = testMeta
    ? { data: testData, meta: testMeta }
    : { ...testData };

  const buildIdStructureProps: buildIdStructureType = {
    idParts: props.idParts ?? defaultIdParts,
    partition: props.partition ?? defaultPartitionParts,
    delimiter: props.delimiter,
  };
  const idStructure = buildIdStructure(buildIdStructureProps);
  expect(idStructure).toStrictEqual(expectedStructure);

  const id = assembleId({
    payload,
    idStructure: idStructure,
  });
  expect(id).toStrictEqual(expectedIdOnTestData);
};

describe("id flow", () => {
  it("returns the occurTime as a default id, if occurTime is present", () => {
    expectStructureAndId(
      {},
      "%occurTime%",
      exampleOccurTime,
      exampleDataOccur,
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
      exampleMeta
    );

    expectStructureAndId(
      {},
      String.raw`%foo%__%bar%__%complex%__%array%__%num%__%wei\%rd%`,
      String.raw`abc__def__{"data":"nested"}__["various",2,"data"]__3__da%ta`,
      exampleData,
      exampleMeta
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
      "%field%:%occurTime%",
      "main:" + exampleOccurTime,
      exampleDataOccurField
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
        idParts: ["%foo", "%?modifyTime", "rawString"],
        delimiter: "__",
        partition: "%field",
      },
      "%field%:%foo%__%?modifyTime%__rawString",
      "main:abc__2020-11-09T00:40:12.544Z__rawString",
      exampleDataOccurField
    );
  });

  it("can use metadata fields by using a question mark before the field", () => {
    expectStructureAndId(
      {
        idParts: "%?modifyTime",
      },
      "%?modifyTime%",
      "2020-11-09T00:40:12.544Z"
    );
    expectStructureAndId(
      { idParts: ["%?modifyTime", "%foo", "%?humanId"] },
      "%?modifyTime%__%foo%__%?humanId%",
      "2020-11-09T00:40:12.544Z__abc__mqp4znq4cvp3qnj74fgi9"
    );
  });

  it("can use a dataField that starts with a question mark by escaping the question", () => {
    expectStructureAndId(
      { idParts: "%\\?modifyTime%" },
      "%\\?modifyTime%",
      "now",
      {
        "?modifyTime": "now",
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
    expect(defaultIdComponents({ data: exampleDataOccur })).toMatchObject({
      defaultIdParts: ["%occurTime%"],
    });
  });

  it("uses a concatenation of data fields if hasOccurTime is false", () => {
    const simpleData = { firstKey: "firstData", secondKey: "secondData" };
    expect(defaultIdComponents({ data: simpleData })).toMatchObject({
      defaultIdParts: ["%firstKey%", "%secondKey%"],
    });
  });

  it("uses field as the default partition", () => {
    expect(
      defaultIdComponents({
        data: { field: "abc", occurTime: exampleOccurTime },
      })
    ).toMatchObject({
      defaultPartitionParts: ["%field%"],
    });
    expect(defaultIdComponents({ data: { field: "abc" } })).toMatchObject({
      defaultPartitionParts: ["%field%"],
    });
    expect(
      defaultIdComponents({
        data: {
          field: "works",
          with: "other",
          keys: "too",
          occurTime: exampleOccurTime,
        },
      })
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
    expect(
      defaultIdComponents({
        data: { field: "works", with: "other", keys: "too" },
      })
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
  });

  it("returns undefined for defaultPartitionParts when no field is present", () => {
    expect(
      defaultIdComponents({
        data: { no: "field", key: "present", occurTime: exampleOccurTime },
      })
    ).toMatchObject({ defaultPartitionParts: undefined });
    expect(
      defaultIdComponents({
        data: { no: "field", key: "present" },
      })
    ).toMatchObject({ defaultPartitionParts: undefined });
  });
});

describe("assembleId", () => {
  it("uses the _id in the payload if no idStructure is provided or found in metadata", () => {
    expect(assembleId({ payload: { _id: "dataOnlyId", foo: "bar" } })).toEqual(
      "dataOnlyId"
    );
    expect(
      assembleId({
        payload: {
          _id: "datumId",
          data: { foo: "bar" },
          meta: { humanId: "does-not-have-id-Structure" },
        },
      })
    );
  });
  it("throws error if no idStructure provided or found, and no _id is in payload", () => {
    expect(() =>
      assembleId({
        payload: {
          data: { abc: "123" },
          meta: { modifyTime: "2020-11-09T00:40:12.544Z" },
        },
      })
    ).toThrowError(IdError);
    expect(() => assembleId({ payload: { abc: "123" } })).toThrowError(IdError);
  });

  it.todo("doesn't allow recursive %?idStructure% as a part of the id");
});

test.todo("it allows for a uuid somehow");

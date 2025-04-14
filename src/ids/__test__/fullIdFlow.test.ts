import {
  DatumData,
  DatumMetadata,
  EitherPayload,
} from "../../documentControl/DatumDocument";

import { buildIdStructure, buildIdStructureType } from "../buildIdStructure";
import { assembleId } from "../assembleId";
import { defaultIdComponents } from "../defaultIdComponents";
import {
  exampleData,
  exampleDataOccur,
  exampleMeta,
  exampleOccurTime,
} from "./exampleData";
import { exampleDataOccurField } from "./exampleData";

function expectStructureAndId(
  props: Partial<buildIdStructureType>,
  expectedStructure: string,
  expectedIdOnTestData: string,
  testData: DatumData = exampleData,
  testMeta: DatumMetadata | false = exampleMeta,
) {
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
}

describe("id flow", () => {
  it("returns the occurTime as a default id, if occurTime is present", () => {
    expectStructureAndId(
      {},
      "%occurTime%",
      exampleOccurTime,
      exampleDataOccur,
      exampleMeta,
    );
  });

  it("uses a concatenation of all fields as the default if no occurTime is present", () => {
    const simpleData: DatumData = { a: 123, b: "abc", three: "value" };
    expectStructureAndId(
      {},
      "%a%__%b%__%three%",
      "123__abc__value",
      simpleData,
      exampleMeta,
    );

    expectStructureAndId(
      {},
      String.raw`%foo%__%bar%__%complex%__%array%__%num%__%wei\%rd%`,
      String.raw`abc__def__{"data":"nested"}__["various",2,"data"]__3__da%ta`,
      exampleData,
      exampleMeta,
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
      "manual_raw$id",
    );
  });

  it("can interpolate raw strings and field names", () => {
    expectStructureAndId({ idParts: "foo%foo%" }, "foo%foo%", "fooabc");
    expectStructureAndId(
      { idParts: "%foo%_:)_%bar%" },
      "%foo%_:)_%bar%",
      "abc_:)_def",
    );
    expectStructureAndId({ idParts: "raw%foo" }, "raw%foo%", "rawabc");
  });

  it("automatically infers a missing trailing %", () => {
    expectStructureAndId({ idParts: "%foo" }, "%foo%", "abc");
    expectStructureAndId({ idParts: "raw%foo" }, "raw%foo%", "rawabc");
    expectStructureAndId(
      { idParts: ["%foo", "%bar"] },
      "%foo%__%bar%",
      "abc__def",
    );
  });

  it("combines multiple components with the id_delimiter", () => {
    expectStructureAndId(
      { idParts: ["%foo", "%bar"] },
      "%foo%__%bar%",
      "abc__def",
    );
    expectStructureAndId(
      { idParts: ["%foo%", "raw", "%bar"] },
      "%foo%__raw__%bar%",
      "abc__raw__def",
    );
    expectStructureAndId(
      { idParts: ["%foo", "%bar"], delimiter: "@" },
      "%foo%@%bar%",
      "abc@def",
    );
    expectStructureAndId(
      { idParts: ["%foo", "%bar"], delimiter: "" },
      "%foo%%bar%",
      "abcdef",
    );
    expectStructureAndId(
      { idParts: ["%foo", "%bar", "rawString"], delimiter: "%" },
      "%foo%\\%%bar%\\%rawString",
      "abc%def%rawString",
    );
  });

  it("omits fields that do not exist", () => {
    expectStructureAndId({ idParts: "%notAField" }, "%notAField%", "");
    expectStructureAndId(
      { idParts: ["%foo", "%notAField", "%bar"] },
      "%foo%__%notAField%__%bar%",
      "abc____def",
    );
  });

  it("can retrieve deeper values", () => {
    expectStructureAndId(
      { idParts: "%complex.data" },
      "%complex.data%",
      "nested",
    );
    expectStructureAndId(
      { idParts: ["%foo", "%complex.notAKey"] },
      "%foo%__%complex.notAKey%",
      "abc__",
    );
    expectStructureAndId({ idParts: "%not.real.keys" }, "%not.real.keys%", "");
  });

  it("serializes numbers, objects, and arrays", () => {
    expectStructureAndId({ idParts: "%num" }, "%num%", "3");
    expectStructureAndId(
      { idParts: "%complex" },
      "%complex%",
      '{"data":"nested"}',
    );
    expectStructureAndId(
      { idParts: "%array" },
      "%array%",
      '["various",2,"data"]',
    );
  });

  it("escapes percent signs properly", () => {
    expectStructureAndId(
      { idParts: "raw with escaped \\%foo" },
      "raw with escaped \\%foo",
      "raw with escaped %foo",
    );
    expectStructureAndId({ idParts: "%wei\\%rd" }, "%wei\\%rd%", "da%ta");
  });

  it("uses field as partition", () => {
    expectStructureAndId(
      {},
      "%occurTime%",
      "main:" + exampleOccurTime,
      exampleDataOccurField,
    );
    expectStructureAndId(
      { idParts: "%foo" },
      "%foo%",
      "otherName:abc",
      { ...exampleData, field: "otherName" },
    );
  });

  it("handles this example", () => {
    expectStructureAndId(
      {
        idParts: ["%foo", "%?modifyTime", "rawString"],
        delimiter: "__",
      },
      "%foo%__%?modifyTime%__rawString",
      "main:abc__2020-11-09T00:40:12.544Z__rawString",
      exampleDataOccurField,
    );
  });

  it("can use metadata fields by using a question mark before the field", () => {
    expectStructureAndId(
      {
        idParts: "%?modifyTime",
      },
      "%?modifyTime%",
      "2020-11-09T00:40:12.544Z",
    );
    expectStructureAndId(
      { idParts: ["%?modifyTime", "%foo", "%?humanId"] },
      "%?modifyTime%__%foo%__%?humanId%",
      "2020-11-09T00:40:12.544Z__abc__mqp4znq4cvp3qnj74fgi9",
    );
  });

  it("can use a dataField that starts with a question mark by escaping the question", () => {
    expectStructureAndId(
      { idParts: "%\\?modifyTime%" },
      "%\\?modifyTime%",
      "now",
      {
        "?modifyTime": "now",
      },
    );
  });

  it("prefers to use the _id in the data, if in no-metadata mode", () => {
    expectStructureAndId(
      { idParts: "%key" },
      "%key%",
      "id-from-data",
      { key: "id-from-idParts", _id: "id-from-data" },
      false,
    );
  });
});

test.todo("it allows for a uuid somehow");

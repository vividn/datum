import { makeDoc } from "../../../__test__/test-utils";
import { toDatumTime } from "../../../time/datumTime";
import * as emit from "../../emit";
import { typeStructureView } from "../typeStructure";

describe("typeStructureView", () => {
  let emitMock: any;
  const tsv = typeStructureView;

  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

  it("emits a key and a type for each top level key", () => {
    const doc = {
      _id: "abcdef",
      _rev: "1-abcdef",
      key1: "stringvalue",
      key2: 36,
      key3: null,
      key4: true,
    };
    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(6);
    expect(emitMock).toHaveBeenCalledWith(["_id", "<string>"], null);
    expect(emitMock).toHaveBeenCalledWith(["_rev", "<string>"], null);
    expect(emitMock).toHaveBeenCalledWith(["key1", "<string>"], null);
    expect(emitMock).toHaveBeenCalledWith(["key2", "<number>"], null);
    expect(emitMock).toHaveBeenCalledWith(["key3", "<null>"], null);
    expect(emitMock).toHaveBeenCalledWith(["key4", "<boolean>"], null);
  });

  it("emits nested keys by lengthening the array, but also high level object type", () => {
    const doc = {
      _id: "abcdef",
      _rev: "1-abcdef",
      data: {
        level2: {
          nested: "stringvalue",
          num: 36,
        },
        key: "value",
      },
    };

    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(7);
    expect(emitMock).toHaveBeenCalledWith(["data", "<object>"], null);
    expect(emitMock).toHaveBeenCalledWith(["data", "level2", "<object>"], null);
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "level2", "nested", "<string>"],
      null,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "level2", "num", "<number>"],
      null,
    );
    expect(emitMock).toHaveBeenCalledWith(["data", "key", "<string>"], null);
  });

  it("emits special types if object implments _t", () => {
    const doc = makeDoc({
      key: "value",
      special: { _t: "specialType", value: "v" },
    });
    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledWith(["data", "key", "<string>"], null);
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "special", "<{specialType}>"],
      null,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "special", "value", "<string>"],
      null,
    );
  });

  it.skip("emits special types for dates, times, etc", () => {
    // TODO: Enable once datumTime has _t implemented
    const doc = makeDoc({
      dur: "PT1H36M",
      occurTime: toDatumTime("2024-08-28 12:45"),
      effectiveDate: { utc: "2024-08-28" },
    });
    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledWith(["data", "dur", "<duration>"], null);
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "occurTime", "<time>"],
      null,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "effectiveDate", "<date>"],
      null,
    );
  });

  it("emits a sorted summary type for arrays", () => {
    const doc = makeDoc({
      arr: ["one", "two", "three"],
      multi: [true, 2, "three"],
      sameSort: ["three", 2, true],
      complexType: [
        "string",
        { value: "v", _t: "a_special_type" },
        { key: "val" },
      ],
    });
    tsv.map(doc);

    expect(emitMock).toHaveBeenCalledWith(["data", "arr", "<string[]>"], null);
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "multi", "<(boolean|number|string)[]>"],
      null,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "sameSort", "<(boolean|number|string)[]>"],
      null,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "complexType", "<(object|string|{a_special_type})[]>"],
      null,
    );
  });

  it("can handle nested arrays", () => {
    const doc = makeDoc({
      simpleNest: [["one"], ["two", "three"]],
      multiNest: [
        ["one", 2, null],
        ["four", "five", "six"],
      ],
    });
    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "simpleNest", "<string[][]>"],
      null,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["data", "multiNest", "<((null|number|string)[]|string[])[]>"],
      null,
    );
  });

  it("does not traverse into objects inside of arrays", () => {
    const doc = {
      _id: "abcdef",
      _rev: "1-abcdef",
      arr: [{ key: "val" }],
    };
    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(3);
    expect(emitMock).toHaveBeenCalledWith(["_id", "<string>"], null);
    expect(emitMock).toHaveBeenCalledWith(["_rev", "<string>"], null);
    expect(emitMock).toHaveBeenCalledWith(["arr", "<object[]>"], null);
  });
});

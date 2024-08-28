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
    expect(emitMock).toHaveBeenCalledWith(["_id", "<string>"]);
    expect(emitMock).toHaveBeenCalledWith(["_rev", "<string>"]);
    expect(emitMock).toHaveBeenCalledWith(["key1", "<string>"]);
    expect(emitMock).toHaveBeenCalledWith(["key2", "<number>"]);
    expect(emitMock).toHaveBeenCalledWith(["key3", "<null>"]);
    expect(emitMock).toHaveBeenCalledWith(["key4", "<boolean>"]);
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
    expect(emitMock).toHaveBeenCalledWith(["data", "<object>"]);
    expect(emitMock).toHaveBeenCalledWith(["data", "level2", "<object>"]);
    expect(emitMock).toHaveBeenCalledWith([
      "data",
      "level2",
      "nested",
      "<string>",
    ]);
    expect(emitMock).toHaveBeenCalledWith([
      "data",
      "level2",
      "num",
      "<number>",
    ]);
    expect(emitMock).toHaveBeenCalledWith(["data", "key", "<string>"]);
  });

  it("emits special types for dates, times, etc", () => {
    const doc = makeDoc({
      dur: "PT1H36M",
      occurTime: toDatumTime("2024-08-28 12:45"),
      effectiveDate: { utc: "2024-08-28" },
    });
    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledWith(["data", "dur", "<duration>"]);
    expect(emitMock).toHaveBeenCalledWith(["data", "occurTime", "<time>"]);
    expect(emitMock).toHaveBeenCalledWith(["data", "effectiveDate", "<date>"]);
  });

  it("emits a sorted summary type for arrays", () => {
    const doc = makeDoc({
      arr: ["one", "two", "three"],
      multi: [true, 2, "three"],
      sameSort: ["three", 2, true],
      complexType: ["string", toDatumTime("2024-08-28 12:45"), { key: "val" }],
    });
    tsv.map(doc);

    expect(emitMock).toHaveBeenCalledWith(["data", "arr", "<string>[]"]);
    expect(emitMock).toHaveBeenCalledWith([
      "data",
      "multi",
      "<boolean|number|string>[]",
    ]);
    expect(emitMock).toHaveBeenCalledWith([
      "data",
      "sameSort",
      "<boolean|number|string>[]",
    ]);
    expect(emitMock).toHaveBeenCalledWith([
      "data",
      "complexType",
      "<string|object|time>[]",
    ]);
  });

  it("does not traverse into objects inside of arrays", () => {
    const doc = {
      _id: "abcdef",
      _rev: "1-abcdef",
      arr: [{ key: "val" }],
    };
    tsv.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(3);
    expect(emitMock).toHaveBeenCalledWith(["_id", "<string>"]);
    expect(emitMock).toHaveBeenCalledWith(["_rev", "<string>"]);
    expect(emitMock).toHaveBeenCalledWith(["arr", "<object>[]"]);
  });
});

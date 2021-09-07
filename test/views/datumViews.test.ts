import {
  afterAll,
  beforeEach,
  describe,
  jest,
  it,
  expect,
} from "@jest/globals";
import * as emit from "../../src/views/emit";
import {
  DataOnlyDocument,
  DatumDocument,
  EitherDocument,
} from "../../src/documentControl/DatumDocument";
import {
  humanIdView,
  subHumanIdView,
} from "../../src/views/datumViews/humanId";
import { dataStructuresView, structuresView } from "../../src/views/datumViews/structure";

const emitMock = jest.spyOn(emit, "default");
beforeEach(() => {
  emitMock.mockClear();
});

afterAll(() => {
  emitMock.mockRestore();
});

describe("humanIdView", () => {
  it("emits one humanId if document has it", () => {
    const doc: DatumDocument = {
      _id: "datum_doc",
      _rev: "some_revision",
      data: {},
      meta: { humanId: "to_be_emitted" },
    };
    humanIdView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith("to_be_emitted", null);
  });

  it("emits nothing without humanId", () => {
    const doc: DatumDocument = {
      _id: "datum_doc",
      _rev: "some_revision",
      data: {},
      meta: {},
    };
    humanIdView.map(doc);
    expect(emitMock).not.toHaveBeenCalled();
  });
});

describe("subHumanIdView", () => {
  it("emits a row for each starting substring of humanId", () => {
    const doc: DatumDocument = {
      _id: "datum_doc",
      _rev: "some_revision",
      data: {},
      meta: { humanId: "substrings" },
    };
    subHumanIdView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(10);
    for (const substr of [
      "s",
      "su",
      "sub",
      "subs",
      "subst",
      "substr",
      "substri",
      "substrin",
      "substring",
      "substrings",
    ]) {
      expect(emitMock).toHaveBeenCalledWith(substr, null);
    }
  });

  it("does not emit if there is no human id", () => {
    const doc: DatumDocument = {
      _id: "datum_doc",
      _rev: "some_revision",
      data: {},
      meta: {},
    };
    subHumanIdView.map(doc);
    expect(emitMock).not.toHaveBeenCalled();
  });
});

describe("structuresView", () => {
  it("emits a sorted array containing each field in a flat document", () => {
    const doc: DataOnlyDocument = {
      _id: "doc",
      _rev: "some_revision",
      abc: 123,
      zed: null,
      def: "baz",
    };
    structuresView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(["abc", "def", "zed"], null);
  });

  it("emits the subkeys (also alphabettically) as well if a field contains an object", () => {
    const doc: DataOnlyDocument = {
      _id: "doc",
      _rev: "some_revision",
      empty: {},
      withData: {
        zdx: "abc",
        someKey: 123,
        nested: {
          even: "further",
        },
      },
      meta: { modifyTime: "2021-09-07T19:26:54.442Z" },
      normalValue: "I'm just a string!",
    };
    structuresView.map(doc);

    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(
      [
        "empty",
        "meta",
        "meta.modifyTime",
        "normalValue",
        "withData",
        "withData.nested",
        "withData.nested.even",
        "withData.someKey",
        "withData.zdx",
      ],
      null
    );
  });
});

describe("dataStructuresView", () => {
  it("emits nothing if there is no data field", () => {
    const doc: EitherDocument = {
      _id: "someId",
      _rev: "some_revision",
      lots: "of interesting",
      keys: "to find",
      butNone: "of them",
      are: "data",
      nested: {
        data: "doesn't count"
      },
      meta: {
        modifyTime: "2021-09-07T19:26:54.442Z"
      }
    };
    dataStructuresView.map(doc);

    expect(emitMock).not.toHaveBeenCalled();
  });

  it("emits the keys and subkeys of data in alphabetical order", () => {
    const doc: EitherDocument = {
      _id: "doc",
      _rev: "some_revision",
      data: {
        empty: {},
        zdx: "abc",
        someKey: 123,
        nested: {
          zzz: "abc",
          aaa: "xxx"
        },
      },
      meta: { modifyTime: "2021-09-07T19:26:54.442Z" },
      externalValue: "I'm ignored!",
    };

    dataStructuresView.map(doc);

    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(["empty", "nested", "nested.aaa",
    "nested.zzz", "someKey", "zdx"], null);
  });
});

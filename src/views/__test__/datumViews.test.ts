import * as emit from "../emit";
import {
  DataOnlyDocument,
  DatumDocument,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { humanIdView, idToHumanView, subHumanIdView } from "../datumViews";
import { dataStructuresView, structuresView } from "../datumViews";

describe("humanIdView", () => {
  let emitMock: any;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

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
  let emitMock: any;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

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

describe("idToHumanView", () => {
  let emitMock: any;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

  it("emits the _id to humanId if it exists", () => {
    const doc: DatumDocument = {
      _id: "some_doc",
      _rev: "some_revision",
      data: {},
      meta: { humanId: "abcde" },
    };
    idToHumanView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith("some_doc", "abcde");
  });

  it("emits nothing if there is no human id", () => {
    const doc: DatumDocument = {
      _id: "some_doc",
      _rev: "some_revision",
      data: {},
      meta: { random: 0.12345 },
    };
    idToHumanView.map(doc);
    expect(emitMock).not.toHaveBeenCalled();
  });
});

describe("structuresView", () => {
  let emitMock: any;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

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
    expect(emitMock).toHaveBeenCalledWith([["abc", "def", "zed"]], null);
  });

  it("emits the subkeys grouped by order (also alphabettically) as well if a field contains an object", () => {
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
        ["empty", "meta", "normalValue", "withData"],
        [
          "meta.modifyTime",
          "withData.nested",
          "withData.someKey",
          "withData.zdx",
        ],
        ["withData.nested.even"],
      ],

      null
    );
  });
});

describe("dataStructuresView", () => {
  let emitMock: any;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

  it("emits nothing if there is no data field", () => {
    const doc: EitherDocument = {
      _id: "someId",
      _rev: "some_revision",
      lots: "of interesting",
      keys: "to find",
      butNone: "of them",
      are: "data",
      nested: {
        data: "doesn't count",
      },
      meta: {
        modifyTime: "2021-09-07T19:26:54.442Z",
      },
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
          aaa: "xxx",
          deeper: {
            value: 1,
          },
        },
      },
      meta: { modifyTime: "2021-09-07T19:26:54.442Z" },
      externalValue: "I'm ignored!",
    };

    dataStructuresView.map(doc);

    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(
      [
        ["empty", "nested", "someKey", "zdx"],
        ["nested.aaa", "nested.deeper", "nested.zzz"],
        ["nested.deeper.value"],
      ],
      null
    );
  });
});

describe("datumV1View", () => {
  let _emitMock: any;
  beforeEach(() => {
    _emitMock = jest.spyOn(emit, "_emit");
  });

  it.todo("does not emit if there is no occurTime");

  it.todo("emits [date, time, offset,...] for a offset of 0");

  it.todo(
    "emits [date, time, offset,...] with Time and Date corrected for the local time based on the offset"
  );

  it.todo("emits duration, if it exists");

  it.todo('emitted duration is "" if duration does not exist');
});

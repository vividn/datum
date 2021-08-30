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
  DatumDocument,
} from "../../src/documentControl/DatumDocument";
import {
  humanIdView,
  subHumanIdView,
} from "../../src/views/datumViews/humanId";

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

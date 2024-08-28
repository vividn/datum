import * as emit from "../../emit";

describe("typeStructureView", () => {
  let emitMock: any;
  const tsv = typeStructureView;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });
})

import * as editInTerminal from "../../utils/editInTerminal";
import SpyInstance = jest.SpyInstance;

describe("editCmd", () => {
  let editInTerminalSpy: SpyInstance;
  beforeAll(() => {
    editInTerminalSpy = jest.spyOn(editInTerminal, "editJSONInTerminal");
  });

  afterAll(() => {
    jest.restoreAllMocks();
  });

  it("calls editInTerminal with the oldDocument in string form", async () => {
    editInTerminalSpy.mockImplementation(async (doc) => doc);
  });
  it("updates the existing document with the edited terminal text");
  it("it fails appropriately if the editor returns an error");
});

import * as editInTerminal from "../../utils/editInTerminal";
import { testDbLifecycle } from "../../__test__/test-utils";
import { editCmd, TooManyToEditError } from "../editCmd";
import { GenericObject } from "../../GenericObject";
import { setupCmd } from "../setupCmd";

describe("editCmd", () => {
  let editJSONInTerminalSpy: any;
  const dbName = "delete_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    editJSONInTerminalSpy = jest.spyOn(editInTerminal, "editJSONInTerminal");
    await setupCmd("");
  });

  it("calls editJSONInTerminal with the oldDocument and returns the new document", async () => {
    editJSONInTerminalSpy.mockImplementation(async (doc: GenericObject) => doc);
    await db.put({ _id: "abcdef", abc: "def" });
    const dbDoc = await db.get("abcdef");

    const returnedDoc = await editCmd("abcdef");
    expect(returnedDoc).toEqual(dbDoc);
  });

  it("updates the existing document with the edited terminal text", async () => {
    const editedDoc = {
      _id: "changed_id",
      def: "ghi",
    };
    editJSONInTerminalSpy.mockImplementation(
      async (_doc: GenericObject) => editedDoc,
    );
    await db.put({ _id: "abcdef", abc: "def" });
    const returnedDoc = await editCmd("abcdef");
    expect(returnedDoc).toMatchObject(editedDoc);
    await expect(db.get("abcdef")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
  });

  it("errors if more than 1 document is passed with a compound quickId", async () => {
    await db.put({
      _id: "abcdef",
      data: { abc: "def" },
      meta: { humanId: "abc" },
    });
    await db.put({
      _id: "ghijkl",
      data: {
        ghi: "jkl",
      },
      meta: { humanId: "jkl" },
    });
    await expect(editCmd(",abc,jkl")).rejects.toThrowError(TooManyToEditError);
  });
});

import * as editInTerminal from "../../utils/editInTerminal";
import { pass, resetTestDb } from "../../test-utils";
import { editCmd } from "../editCmd";
import { GenericObject } from "../../GenericObject";
import { EitherPayload } from "../../documentControl/DatumDocument";

describe("editCmd", () => {
  let editJSONInTerminalSpy: any;
  const dbName = "delete_cmd_test";
  let db: PouchDB.Database<EitherPayload>;

  beforeEach(async () => {
    db = await resetTestDb(dbName);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  beforeEach(async () => {
    editJSONInTerminalSpy = jest.spyOn(editInTerminal, "editJSONInTerminal");
  });

  it("calls editJSONInTerminal with the oldDocument and returns the new document", async () => {
    editJSONInTerminalSpy.mockImplementation(async (doc: GenericObject) => doc);
    await db.put({ _id: "abcdef", abc: "def" });
    const dbDoc = await db.get("abcdef");

    const returnedDoc = await editCmd({ db: dbName, quickId: "abcdef" });
    expect(returnedDoc).toEqual(dbDoc);
  });

  it("updates the existing document with the edited terminal text", async () => {
    const editedDoc = {
      _id: "changed_id",
      def: "ghi",
    };
    editJSONInTerminalSpy.mockImplementation(
      async (_doc: GenericObject) => editedDoc
    );
    await db.put({ _id: "abcdef", abc: "def" });
    const returnedDoc = await editCmd({ db: dbName, quickId: "abcdef" });
    expect(returnedDoc).toMatchObject(editedDoc);
    await expect(db.get("abcdef")).rejects.toThrowErrorMatchingInlineSnapshot(
      `"deleted"`
    );
  });
});

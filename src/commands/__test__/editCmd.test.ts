import * as editInTerminal from "../../utils/editInTerminal";
import { afterAll, afterEach, beforeEach, jest } from "@jest/globals";
import { pass, resetTestDb, testNano } from "../../test-utils";
import { DocumentScope } from "nano";
import { EitherPayload } from "../../documentControl/DatumDocument";
import * as connectDb from "../../auth/connectDb";
import { editCmd } from "../editCmd";

describe("editCmd", () => {
  let editJSONInTerminalSpy: any;
  const dbName = "delete_cmd_test";
  const db = testNano.use(dbName) as DocumentScope<EitherPayload>;
  const connectDbSpy = jest
    .spyOn(connectDb, "default")
    .mockImplementation(() => db);

  beforeEach(async () => {
    await resetTestDb(dbName);
    editJSONInTerminalSpy = jest.spyOn(editInTerminal, "editJSONInTerminal");
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  afterAll(async () => {
    connectDbSpy.mockRestore();
    jest.restoreAllMocks();
  });

  it("calls editJSONInTerminal with the oldDocument and returns the new document", async () => {
    editJSONInTerminalSpy.mockImplementation(async (doc) => doc);
    await db.insert({ _id: "abcdef", abc: "def" });
    const dbDoc = await db.get("abcdef");

    const returnedDoc = await editCmd({ db: dbName, quickId: "abcdef" });
    expect(returnedDoc).toEqual(dbDoc);
  });

  it("updates the existing document with the edited terminal text", async () => {
    const editedDoc = {
      _id: "changed_id",
      def: "ghi",
    };
    editJSONInTerminalSpy.mockImplementation(async (_doc) => editedDoc);
    await db.insert({ _id: "abcdef", abc: "def" });
    const returnedDoc = await editCmd({ db: dbName, quickId: "abcdef" });
    expect(returnedDoc).toMatchObject(editedDoc);
    await expect(db.get("abcdef")).rejects.toThrowError("deleted");
  });
});

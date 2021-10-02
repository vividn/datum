import { afterAll, beforeEach, expect, it, jest } from "@jest/globals";
import { resetTestDb, testNano } from "../test-utils";
import setupCmd from "../../src/commands/setupCmd";
import * as updateDoc from "../../src/documentControl/updateDoc";
import { DocumentScope } from "nano";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import { updateCmd } from "../../src/commands/updateCmd";

const dbName = "update_cmd_test";
const db = testNano.use<EitherPayload>(dbName);

const updateDocSpy = jest.spyOn(updateDoc, "default");

// beforeEach(async () => {
//   await resetTestDb(dbName);
//   await setupCmd({ db: dbName });
//   updateDocSpy.mockClear();
// });
//
// afterAll(async () => {
//   await testNano.db.destroy(dbName);
//   updateDocSpy.mockRestore();
// });

// it("can update an existing doc from the first few letters of its humanId", async () => {
//   await db.insert({_id: "doc_to_update", data: {foo: "bar"}, meta: {humanId: "abcdefg"}});
//   const retDoc = await updateCmd({db: dbName, quickId: "abc", strategy: "preferNew", data: ["foo=baz", "newField=newData"]});
//   const dbDoc = await db.get("doc_to_update");
//   expect(retDoc).toEqual(dbDoc);
//   expect(retDoc).toMatchObject({_id: "doc_to_update", data: {foo: "baz", newField: "newData"}});
// });

it.todo("can update a datonly doc from the first letters of its id");

it.todo("calls quickId and updateDoc");

it.todo("uses preferNew as the default updateStrategy");

it.todo("outputs an UPDATE message or a NODIFF message when show is standard");

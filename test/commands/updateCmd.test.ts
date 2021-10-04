import { afterAll, beforeEach, expect, it, jest } from "@jest/globals";
import { resetTestDb, testNano } from "../test-utils";
import setupCmd from "../../src/commands/setupCmd";
import * as updateDoc from "../../src/documentControl/updateDoc";
import {
  EitherDocument,
  EitherPayload,
} from "../../src/documentControl/DatumDocument";
import { updateCmd } from "../../src/commands/updateCmd";
import * as quickId from "../../src/ids/quickId";
import { Show } from "../../src/output";
import { mock } from "jest-mock-extended";

const dbName = "update_cmd_test";
const db = testNano.use<EitherPayload>(dbName);

beforeEach(async () => {
  await resetTestDb(dbName);
  await setupCmd({ db: dbName });
});

afterAll(async () => {
  await testNano.db.destroy(dbName);
});

it("can update an existing doc from the first few letters of its humanId", async () => {
  await db.insert({
    _id: "doc_to_update",
    data: { foo: "bar" },
    meta: { humanId: "abcdefg" },
  });
  const retDoc = await updateCmd({
    db: dbName,
    quickId: "abc",
    strategy: "preferNew",
    data: ["foo=baz", "newField=newData"],
  });
  const dbDoc = await db.get("doc_to_update");
  expect(retDoc).toEqual(dbDoc);
  expect(retDoc).toMatchObject({
    _id: "doc_to_update",
    data: { foo: "baz", newField: "newData" },
  });
});

it("can update a datonly doc from the first letters of its id", async () => {
  await db.insert({ _id: "some_data_only", foo: "bar" });
  const retDoc = await updateCmd({
    db: dbName,
    quickId: "some",
    strategy: "merge",
    required: ["foo"],
    optional: ["newField"],
    data: ["baz", "newData"],
  });
  const dbDoc = await db.get("some_data_only");
  expect(retDoc).toEqual(dbDoc);
  expect(retDoc).toMatchObject({
    _id: "some_data_only",
    foo: ["bar", "baz"],
    newField: "newData",
  });
});

it("calls quickId and updateDoc", async () => {
  const updateDocReturn = mock<EitherDocument>();
  const quickIdSpy = jest
    .spyOn(quickId, "default")
    .mockImplementation(async () => "quick_id");
  const updateDocSpy = jest
    .spyOn(updateDoc, "default")
    .mockReturnValue(Promise.resolve(updateDocReturn));

  const retDoc = await updateCmd({
    db: dbName,
    quickId: "input_quick",
    strategy: "xor",
    data: ["foo=bar"],
  });
  expect(retDoc).toBe(updateDocReturn);
  expect(quickIdSpy).toHaveBeenCalledWith(expect.anything(), "input_quick");
  expect(updateDocSpy).toHaveBeenCalledWith(
    expect.objectContaining({
      id: "quick_id",
      updateStrategy: "xor",
      payload: { foo: "bar" },
    })
  );

  updateDocSpy.mockRestore();
  quickIdSpy.mockRestore();
});

it("uses preferNew as the default updateStrategy", async () => {
  const quickIdSpy = jest
    .spyOn(quickId, "default")
    .mockImplementation(async () => "quick_id");
  const updateDocSpy = jest
    .spyOn(updateDoc, "default")
    .mockReturnValue(Promise.resolve(mock<EitherDocument>()));

  await updateCmd({ db: dbName, quickId: "input_quick", data: ["foo=bar"] });
  expect(updateDocSpy).toHaveBeenCalledWith(
    expect.objectContaining({ updateStrategy: "preferNew" })
  );

  updateDocSpy.mockRestore();
  quickIdSpy.mockRestore();
});

it("outputs an UPDATE message or a NODIFF message when show is standard", async () => {
  const originalLog = console.log;
  const mockLog = jest.fn();
  console.log = mockLog;

  await db.insert({ _id: "zzz", data: { foo: "bar" }, meta: {} });
  await updateCmd({
    db: dbName,
    quickId: "zzz",
    data: ["foo=baz"],
    show: Show.Standard,
  });
  expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
  mockLog.mockReset();

  await updateCmd({
    db: dbName,
    quickId: "zzz",
    data: ["foo=baz"],
    show: Show.Standard,
  });
  expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));

  console.log = originalLog;
});

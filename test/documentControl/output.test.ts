import { afterEach, beforeEach, expect, jest, test } from "@jest/globals";
import updateDoc from "../../src/documentControl/updateDoc";
import * as updateDocModule from "../../src/documentControl/updateDoc";
import { fail, resetTestDb, testNano } from "../test-utils";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import addDoc from "../../src/documentControl/addDoc";
import { DocExistsError } from "../../src/documentControl/base";
import overwriteDoc from "../../src/documentControl/overwriteDoc";
import { Show } from "../../src/output";
import * as addDocModule from "../../src/documentControl/addDoc";
import addCmd from "../../src/commands/addCmd";
import { main } from "../../src";
import deleteDoc from "../../src/documentControl/deleteDoc";

const dbName = "doc_control_output_test";
const db = testNano.db.use<EitherPayload>(dbName);
const originalLog = console.log;
const mockedLog = jest.fn();

beforeEach(async () => {
  await resetTestDb(dbName);
  mockedLog.mockReset();
  console.log = mockedLog;
});

afterEach(async () => {
  await testNano.db.destroy(dbName);
  console.log = originalLog;
});

test("addDoc displays a CREATE: message and the document if showOutput", async () => {
  await addDoc({
    db,
    payload: { _id: "added-doc", abc: "def" },
    show: Show.Standard,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
});

test("addDoc displays an EXISTS: and FAILED: message if showOuput and conlfict", async () => {
  await db.insert({ _id: "alreadyHere", foo: "bar" });
  try {
    await addDoc({
      db,
      payload: { _id: "alreadyHere", baz: "bazzy" },
      show: Show.Standard,
    });
    fail();
  } catch (e) {
    expect(e).toBeInstanceOf(DocExistsError);
  }

  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
});

test("addDoc calls updateDoc with showOutput", async () => {
  const spy = jest.spyOn(updateDocModule, "default");
  const payload = { _id: "docId", foo: "abce" };
  await addDoc({ db, payload, show: Show.Standard, conflictStrategy: "merge" });
  expect(spy).not.toHaveBeenCalled();

  mockedLog.mockClear();

  await addDoc({ db, payload, show: Show.Standard, conflictStrategy: "merge" });
  expect(spy.mock.calls[0][0].show).toEqual(Show.Standard);
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));

  mockedLog.mockClear();

  await addDoc({
    db,
    payload: { ...payload, extraKey: 123 },
    show: Show.Standard,
    conflictStrategy: "merge",
  });
  expect(spy.mock.calls[1][0].show).toEqual(Show.Standard);
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));

  spy.mockRestore();
});

test("updateDoc outputs an UPDATE: message if showOutput is true", async () => {
  await db.insert({ _id: "name", foo: "bar" });
  await updateDoc({
    db,
    id: "name",
    payload: { foo2: "abc2" },
    show: Show.Standard,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
});

test("updateDoc outputs a NODIFF: message if showing output and no update needed", async () => {
  await db.insert({ _id: "name", foo: "bar" });
  await updateDoc({
    db,
    id: "name",
    payload: { foo2: "abc2" },
    updateStrategy: "useOld",
    show: Show.Standard,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
});

test("updateDoc outputs a RENAME: UPDATE:", async () => {
  await db.insert({
    _id: "docId",
    foo: "abc",
  });
  await updateDoc({
    db,
    id: "docId",
    payload: { _id: "newId", foo: "bar" },
    updateStrategy: "preferNew",
    show: Show.Standard,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("RENAME"));
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
});

test("updateDoc throws and outputs an EXISTS: FAILED:", async () => {
  await db.insert({
    _id: "docId",
    foo: "abc",
  });
  await db.insert({ _id: "conflictId", some: "data" });
  try {
    await updateDoc({
      db,
      id: "docId",
      payload: { _id: "conflictId", foo: "bar" },
      updateStrategy: "preferNew",
      show: Show.Standard,
    });
    fail();
  } catch (e) {
    expect(e).toBeInstanceOf(DocExistsError);
  }
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
});

test("overwriteDoc outputs OWRITE", async () => {
  await db.insert({
    _id: "docId",
    data: { foo: "abc" },
    meta: { humanId: "abcd" },
  });
  await overwriteDoc({
    db,
    id: "docId",
    payload: { _id: "docId", data: { bar: "def" }, meta: { humanId: "defg" } },
    show: Show.Standard,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("RENAME"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
});

test("overwriteDoc outputs a RENAME: OWRITE:", async () => {
  await db.insert({
    _id: "docId",
    data: { foo: "abc" },
    meta: { humanId: "abcd" },
  });
  await overwriteDoc({
    db,
    id: "docId",
    payload: { _id: "newId", data: { bar: "def" }, meta: { humanId: "defg" } },
    show: Show.Standard,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("RENAME"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
});

test("overwriteDoc outputs NODIFF", async () => {
  await db.insert({
    _id: "docId",
    data: { foo: "abc" },
    meta: { humanId: "abcd" },
  });
  await overwriteDoc({
    db,
    id: "docId",
    payload: { _id: "docId", data: { foo: "abc" }, meta: { humanId: "abcd" } },
    show: Show.Standard,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
});

test("overwriteDoc throws and outputs an EXISTS: FAILED:", async () => {
  await db.insert({
    _id: "docId",
    data: { foo: "abc" },
    meta: { humanId: "abcd" },
  });
  await db.insert({
    _id: "conflictId",
    data: { some: "data" },
    meta: { humanId: "conlfict" },
  });
  try {
    await overwriteDoc({
      db,
      id: "docId",
      payload: {
        _id: "conflictId",
        data: { bar: "def" },
        meta: { humanId: "defg" },
      },
      show: Show.Standard,
    });
    fail();
  } catch (e) {
    expect(e).toBeInstanceOf(DocExistsError);
  }
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("RENAME"));
});

test("deleteDoc outputs DELETE", async () => {
  await db.insert({ _id: "doc-to-delete" });
  await deleteDoc({ db, id: "doc-to-delete", show: Show.Standard });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("DELETE"));
});

test("show is None by default when calling a command via import or API", async () => {
  const spy = jest.spyOn(addDocModule, "default").mockReturnValue(
    Promise.resolve({
      _id: "returnDoc",
      _rev: "1-abcd",
      data: {},
      meta: {},
    })
  );
  await addCmd({});
  expect(spy.mock.calls[0][0].show).toEqual(Show.None);
  spy.mockRestore();
});


// This test breaks because yargs can't seem to handle the jest environment now even when just parsing strings
test.skip("show is Standard by default when calling from the CLI", async () => {
  const spy = jest.spyOn(addDocModule, "default").mockReturnValue(
    Promise.resolve({
      _id: "returnDoc",
      _rev: "1-abcd",
      data: {},
      meta: {},
    })
  );

  await main(["--db", dbName, "add"]);
  expect(spy).toHaveBeenCalled();
  expect(spy.mock.calls[0][0].show).toEqual(Show.Standard);

  spy.mockRestore();
});

import {
  afterEach,
  beforeAll,
  beforeEach,
  expect,
  jest,
  test,
} from "@jest/globals";
import updateDoc from "../../src/documentControl/updateDoc";
import * as updateDocModule from "../../src/documentControl/updateDoc";
import { fail, pass, testNano } from "../test-utils";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import addDoc from "../../src/documentControl/addDoc";
import { DocExistsError } from "../../src/documentControl/base";
import overwriteDoc from "../../src/documentControl/overwriteDoc";

const dbName = "doc_control_output_test";
const db = testNano.db.use<EitherPayload>(dbName);
const originalLog = console.log;
const mockedLog = jest.fn();

beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
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
    showOutput: true,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
});

test("addDoc displays an EXISTS: and FAILED: message if showOuput and conlfict", async () => {
  await db.insert({ _id: "alreadyHere", foo: "bar" });
  try {
    await addDoc({
      db,
      payload: { _id: "alreadyHere", baz: "bazzy" },
      showOutput: true,
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
  await addDoc({ db, payload, showOutput: true, conflictStrategy: "merge" });
  expect(spy).not.toHaveBeenCalled();

  mockedLog.mockClear();

  await addDoc({ db, payload, showOutput: true, conflictStrategy: "merge" });
  expect(spy.mock.calls[0][0].showOutput).toEqual(true);
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));

  mockedLog.mockClear();

  await addDoc({
    db,
    payload: { ...payload, extraKey: 123 },
    showOutput: true,
    conflictStrategy: "merge",
  });
  expect(spy.mock.calls[1][0].showOutput).toEqual(true);
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));

  spy.mockRestore();
});

test("updateDoc outputs an UPDATE: message if showOutput is true", async () => {
  await db.insert({ _id: "name", foo: "bar" });
  await updateDoc({
    db,
    id: "name",
    payload: { foo2: "abc2" },
    showOutput: true,
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
    showOutput: true,
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
});

test.todo("updateDoc outputs a RENAME: UPDATE:", async () => {
  await db.insert({
    _id: "docId",
    foo: "abc",
  });
  await updateDoc({
    db,
    id: "docId",
    payload: { _id: "newId", foo: "bar" },
    updateStrategy: "preferNew",
    showOutput: true,
  });
});

test.todo("updateDoc throws and outputs an EXISTS: FAIL!!:");

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
    showOutput: true
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
    showOutput: true
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
    showOutput: true
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
});

test("overwriteDoc throws and outputs an EXISTS: FAIL!!:", async () => {
  await db.insert({
    _id: "docId",
    data: { foo: "abc" },
    meta: { humanId: "abcd" },
  });
  await db.insert({
    _id: "conflictDoc",
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
      showOutput: true
    });
    fail();
  } catch (e) {
    expect(e).toBeInstanceOf(DocExistsError);
  }
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAIL!!"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
  expect(mockedLog).not.toHaveBeenCalledWith(expect.stringContaining("RENAME"));
});

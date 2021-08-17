import {
  afterEach,
  beforeAll,
  beforeEach,
  expect,
  jest,
  test,
} from "@jest/globals";
import updateDoc from "../../src/documentControl/updateDoc";
import { pass, testNano } from "../test-utils";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import addDoc from "../../src/documentControl/addDoc";

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
  await addDoc({ db, payload: { abc: "def" }, showOutput: true });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
});

test("addDoc displays an EXISTS: message and the document if showOuput and conlfict", async () => {
  await db.insert({ _id: "alreadyHere", foo: "bar" });
  await addDoc({
    db,
    payload: { _id: "alreadyHere", baz: "bazzy" },
    showOutput: true,
  }).catch(pass);
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
});

test("updateDoc outputs an UPDATE: message if showOutput is true", async () => {
  await db.insert({ _id: "name", foo: "bar" });
  await updateDoc({ db, id: "name", payload: { foo2: "abc2" } });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
});

test("updateDoc outputs a NODIFF: message if showing output and no update needed", async () => {
  await db.insert({ _id: "name", foo: "bar" });
  await updateDoc({
    db,
    id: "name",
    payload: { foo2: "abc2" },
    updateStrategy: "useOld",
  });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
});

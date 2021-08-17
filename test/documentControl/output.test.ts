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

test.todo("addDoc displays a CREATE: message and the document if showOutput");
test.todo(
  "addDoc displays an EXISTS: message and the document if showOuput and conlfict"
);

test("updateDoc outputs an UPDATE: message if showOutput is true", async () => {
  await db.insert({ _id: "name", foo: "bar" });
  await updateDoc({ db, id: "name", payload: { foo2: "abc2" } });
  expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
});

test.todo(
  "updateDoc outputs a NODIFF: message if showing output and no update needed",
  async () => {
    await db.insert({ _id: "name", foo: "bar" });
    await updateDoc({
      db,
      id: "name",
      payload: { foo2: "abc2" },
      updateStrategy: "useOld",
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
  }
);

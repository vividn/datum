import {
  afterEach,
  beforeAll,
  beforeEach,
  test,
  jest,
  expect,
} from "@jest/globals";
import { fail, pass, testNano } from "../test-utils";
import insertDatumView from "../../src/views/insertDatumView";
import { idToHumanView, subHumanIdView } from "../../src/views/datumViews";
import { DocumentScope } from "nano";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import quickId, { AmbiguousQuickIdError } from "../../src/ids/quickId";

const dbName = "test_quick_id";
const db: DocumentScope<EitherPayload> = testNano.use(dbName);

beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
  await insertDatumView({db, datumView: idToHumanView});
  await insertDatumView({db, datumView: subHumanIdView});
});

afterEach(async () => {
  await testNano.db.destroy(dbName).catch(pass);
  jest.restoreAllMocks();
});

test("it returns the string directly if the exact id exists in the database", async () => {
  await db.insert({_id: "exact-id", data: {}, meta: {}});
  const quick = await quickId(db, "exact-id");
  expect(quick).toBe("exact-id");
});

test("if the text matches the beginning of exactly one humanId, it returns the associated _id", async () => {
  await db.insert({_id: "doc-id1", data: {}, meta: {humanId: "abcdefg"}});
  await db.insert({_id: "doc-id2", data: {}, meta: {humanId: "abzzzzz"}});

  const quick = await quickId(db, "abc");
  expect(quick).toBe("doc-id1");
});

test("if the text matches more than one humanId, it throws an error, showing the possible matches", async () => {
  await db.insert({_id: "doc-id1", data: {}, meta: {humanId: "abcdefg"}});
  await db.insert({_id: "doc-id2", data: {}, meta: {humanId: "abzzzzz"}});

  try {
    await quickId(db, "ab");
    fail();
  } catch (error) {
    expect(error).toBeInstanceOf(AmbiguousQuickIdError);
    const message = (error as AmbiguousQuickIdError).message;
    expect(message).toMatch(/\Wabc\W/);
    expect(message).toMatch(/\Wabz\W/);
    expect(message).toMatch(/\Wdoc-id1\W/);
    expect(message).toMatch(/\Wdoc-id2\W/);
  }
});

test("if no human ids match, and string matches the beginning of exactly one _id, return that _id", async () => {
  await db.insert({_id: "zzz_this_id", data: {}, meta: {humanId: "abcdefg"}});
  await db.insert({_id: "xxx_another", data: {}, meta: {humanId: "dfghrtoi"}});
  await db.insert({_id: "yyy_finally", data: {}, meta: {humanId: "no-matches-here"}});

  const quick = await quickId(db, "zzz");
  expect(quick).toBe("zzz_this_id");
});

test.todo("no humanIds match, but several _ids match starting sub string, throw error and show possible matches");
test.todo("if the substring starts both an id and a human id, then it prefers the humanId");
test.todo("if the string matches an id exactly and a human id exactly, prefer the id match");
test.todo("if the string matches an id exactly and is the begining of one or more human ids, prefer the id");
test.todo("if no humanIds or ids match at all, throw an error");
test.todo("it raises MissingDatumViewError if one of the datumviews is missing");
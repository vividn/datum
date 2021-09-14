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
import quickId, {
  AmbiguousQuickIdError,
  NoQuickIdMatchError,
} from "../../src/ids/quickId";
import { DatumViewMissingError } from "../../src/errors";

const dbName = "test_quick_id";
const db: DocumentScope<EitherPayload> = testNano.use(dbName);

beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
  await insertDatumView({ db, datumView: idToHumanView });
  await insertDatumView({ db, datumView: subHumanIdView });
});

afterEach(async () => {
  await testNano.db.destroy(dbName).catch(pass);
  jest.restoreAllMocks();
});

test("it returns the string directly if the exact id exists in the database", async () => {
  await db.insert({ _id: "exact-id", data: {}, meta: {} });
  const quick = await quickId(db, "exact-id");
  expect(quick).toBe("exact-id");
});

test("if the text matches the beginning of exactly one humanId, it returns the associated _id", async () => {
  await db.insert({ _id: "doc-id1", data: {}, meta: { humanId: "abcdefg" } });
  await db.insert({ _id: "doc-id2", data: {}, meta: { humanId: "abzzzzz" } });

  const quick = await quickId(db, "abc");
  expect(quick).toBe("doc-id1");
});

test("if the text matches more than one humanId, it throws an error, showing the possible matches", async () => {
  await db.insert({ _id: "doc-id1", data: {}, meta: { humanId: "abcdefg" } });
  await db.insert({ _id: "doc-id2", data: {}, meta: { humanId: "abzzzzz" } });

  try {
    await quickId(db, "ab");
    fail();
  } catch (error) {
    expect(error).toBeInstanceOf(AmbiguousQuickIdError);
    const message = (error as AmbiguousQuickIdError).message;
    expect(message).toMatch(/\Wabc\W/);
    expect(message).toMatch(/\Wdoc-id1\W/);
    expect(message).toMatch(/\Wabz\W/);
    expect(message).toMatch(/\Wdoc-id2\W/);
  }
});

test("if no human ids match, and string matches the beginning of exactly one _id, return that _id", async () => {
  await db.insert({
    _id: "zzz_this_id",
    data: {},
    meta: { humanId: "abcdefg" },
  });
  await db.insert({
    _id: "xxx_another",
    data: {},
    meta: { humanId: "dfghrtoi" },
  });
  await db.insert({
    _id: "yyy_finally",
    data: {},
    meta: { humanId: "no-matches-here" },
  });

  const quick = await quickId(db, "zzz");
  expect(quick).toBe("zzz_this_id");
});

test("if no humanIds match, but several _ids match starting sub string, throw error and show possible matches", async () => {
  await db.insert({
    _id: "zzz_this_id",
    data: {},
    meta: { humanId: "abcdefg" },
  });
  await db.insert({
    _id: "zzz_same_start",
    data: {},
    meta: { humanId: "dfghrtoi" },
  });
  await db.insert({
    _id: "yyy_finally",
    data: {},
    meta: { humanId: "no-matches-here" },
  });

  try {
    await quickId(db, "zzz");
    fail();
  } catch (error) {
    expect(error).toBeInstanceOf(AmbiguousQuickIdError);
    const message = (error as AmbiguousQuickIdError).message;
    expect(message).toMatch(/\Wa\W/);
    expect(message).toMatch(/\Wzzz_this_id\W/);
    expect(message).toMatch(/\Wd\W/);
    expect(message).toMatch(/\Wzzz_same_start\W/);
  }
});

test("if the substring starts both an id and a human id, then it prefers the humanId", async () => {
  await db.insert({
    _id: "abc_this_is_an_id",
    data: {},
    meta: { humanId: "xyz_human_for_me" },
  });
  await db.insert({
    _id: "another_id",
    data: {},
    meta: { humanId: "abcdefg" },
  });

  const quick = await quickId(db, "abc");
  expect(quick).toBe("another_id");
});

test("if the string matches an id exactly and a human id exactly, prefer the id match", async () => {
  await db.insert({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });
  await db.insert({
    _id: "human_is_like_other_id",
    data: {},
    meta: { humanId: "abcId" },
  });

  const quick = await quickId(db, "abcId");
  expect(quick).toBe("abcId");
});

test("if the string matches an id exactly and is the beginning of one or more human ids, prefer the id", async () => {
  await db.insert({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });
  await db.insert({
    _id: "human_starts_like_other_id",
    data: {},
    meta: { humanId: "abcId_extra" },
  });

  const quick = await quickId(db, "abcId");
  expect(quick).toBe("abcId");

  await db.insert({
    _id: "another_one_with_extra_human",
    data: {},
    meta: { humanId: "abcId_and_more" },
  });

  const quick2 = await quickId(db, "abcId");
  expect(quick2).toBe("abcId");
});

test("if no humanIds or ids match at all, throw a NoQuickIdMatchError", async () => {
  await db.insert({
    _id: "zzz_this_id",
    data: {},
    meta: { humanId: "abcdefg" },
  });
  await db.insert({
    _id: "xxx_another",
    data: {},
    meta: { humanId: "dfghrtoi" },
  });
  await db.insert({
    _id: "yyy_finally",
    data: {},
    meta: { humanId: "no-matches-here" },
  });

  await expect(() => quickId(db, "lmnop")).rejects.toThrowError(
    NoQuickIdMatchError
  );
});

test("it raises DatumViewMissingError if idToHumanView is missing", async () => {
  const viewDoc = await db.get("_design/" + idToHumanView.name);
  await db.destroy("_design/" + idToHumanView.name, viewDoc._rev);

  await db.insert({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });

  await expect(() => quickId(db, "hum")).rejects.toThrowError(
    DatumViewMissingError
  );
});

test("it raises DatumViewMissingError if subHumanIdView is missing", async () => {
  const viewDoc = await db.get("_design/" + subHumanIdView.name);
  await db.destroy("_design/" + subHumanIdView.name, viewDoc._rev);

  await db.insert({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });

  await expect(() => quickId(db, "hum")).rejects.toThrowError(
    DatumViewMissingError
  );
});

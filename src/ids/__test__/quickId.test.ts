import { fail, testDbLifecycle } from "../../test-utils";
import { insertDatumView } from "../../views/insertDatumView";
import {
  humanIdView,
  idToHumanView,
  subHumanIdView,
} from "../../views/datumViews";
import {
  quickId,
  AmbiguousQuickIdError,
  NoQuickIdMatchError,
  quickIds,
} from "../quickId";

jest.retryTimes(3);

describe("quickId", () => {
  const dbName = "test_quick_id";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await insertDatumView({ db, datumView: idToHumanView });
    await insertDatumView({ db, datumView: subHumanIdView });
    await insertDatumView({ db, datumView: humanIdView });
  });

  test("it returns the string directly if the exact id exists in the database", async () => {
    await db.put({ _id: "exact-id", data: {}, meta: {} });
    const quick = await quickId(db, "exact-id");
    expect(quick).toBe("exact-id");
  });

  test("if the text matches the beginning of exactly one humanId, it returns the associated _id", async () => {
    await db.put({ _id: "doc-id1", data: {}, meta: { humanId: "abcdefg" } });
    await db.put({ _id: "doc-id2", data: {}, meta: { humanId: "abzzzzz" } });

    const quick = await quickId(db, "abc");
    expect(quick).toBe("doc-id1");
  });

  test("if the text matches more than one humanId, it throws an error, showing the possible matches", async () => {
    await db.put({ _id: "doc-id1", data: {}, meta: { humanId: "abcdefg" } });
    await db.put({ _id: "doc-id2", data: {}, meta: { humanId: "abzzzzz" } });

    try {
      await quickId(db, "ab");
      fail();
    } catch (error) {
      expect(error).toBeInstanceOf(AmbiguousQuickIdError);
      const message = (error as AmbiguousQuickIdError).message;
      expect(message).toMatch(/\babc\b/);
      expect(message).toMatch(/\bdoc-id1\b/);
      expect(message).toMatch(/\babz\b/);
      expect(message).toMatch(/\bdoc-id2\b/);
    }
  });

  test("if no human ids match, and string matches the beginning of exactly one _id, return that _id", async () => {
    await db.put({
      _id: "zzz_this_id",
      data: {},
      meta: { humanId: "abcdefg" },
    });
    await db.put({
      _id: "xxx_another",
      data: {},
      meta: { humanId: "dfghrtoi" },
    });
    await db.put({
      _id: "yyy_finally",
      data: {},
      meta: { humanId: "no-matches-here" },
    });

    const quick = await quickId(db, "zzz");
    expect(quick).toBe("zzz_this_id");
  });

  test("if no humanIds match, but several _ids match starting sub string, throw error and show possible matches", async () => {
    await db.put({
      _id: "zzz_this_id",
      data: {},
      meta: { humanId: "abcdefg" },
    });
    await db.put({
      _id: "zzz_same_start",
      data: {},
      meta: { humanId: "dfghrtoi" },
    });
    await db.put({
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
      expect(message).toMatch(/\ba\b/);
      expect(message).toMatch(/\bzzz_this_id\b/);
      expect(message).toMatch(/\bd\b/);
      expect(message).toMatch(/\bzzz_same_start\b/);
    }
  });

  test("if the substring starts both an id and a human id, then it prefers the humanId", async () => {
    await db.put({
      _id: "abc_this_is_an_id",
      data: {},
      meta: { humanId: "xyz_human_for_me" },
    });
    await db.put({
      _id: "another_id",
      data: {},
      meta: { humanId: "abcdefg" },
    });

    const quick = await quickId(db, "abc");
    expect(quick).toBe("another_id");
  });

  test("if the string matches an id exactly and a human id exactly, prefer the id match", async () => {
    await db.put({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });
    await db.put({
      _id: "human_is_like_other_id",
      data: {},
      meta: { humanId: "abcId" },
    });

    const quick = await quickId(db, "abcId");
    expect(quick).toBe("abcId");
  });

  test("if the string matches an id exactly and is the beginning of one or more human ids, prefer the id", async () => {
    await db.put({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });
    await db.put({
      _id: "human_starts_like_other_id",
      data: {},
      meta: { humanId: "abcId_extra" },
    });

    const quick = await quickId(db, "abcId");
    expect(quick).toBe("abcId");

    await db.put({
      _id: "another_one_with_extra_human",
      data: {},
      meta: { humanId: "abcId_and_more" },
    });

    const quick2 = await quickId(db, "abcId");
    expect(quick2).toBe("abcId");
  });

  test("if no humanIds or ids match at all, throw a NoQuickIdMatchError", async () => {
    await db.put({
      _id: "zzz_this_id",
      data: {},
      meta: { humanId: "abcdefg" },
    });
    await db.put({
      _id: "xxx_another",
      data: {},
      meta: { humanId: "dfghrtoi" },
    });
    await db.put({
      _id: "yyy_finally",
      data: {},
      meta: { humanId: "no-matches-here" },
    });

    await expect(() => quickId(db, "lmnop")).rejects.toThrowError(
      NoQuickIdMatchError
    );
  });
});

describe("quickIds", () => {
  const dbName = "test_quick_ids";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await insertDatumView({ db, datumView: idToHumanView });
    await insertDatumView({ db, datumView: subHumanIdView });
    await insertDatumView({ db, datumView: humanIdView });
    await db.put({
      _id: "id1",
      meta: { humanId: "abc" },
    });
    await db.put({
      _id: "id2",
      meta: { humanId: "ghi" },
    });
    await db.put({
      _id: "id3",
      meta: { humanId: "jkl" },
    });
  });

  it("can take a comma separated list of quick ids that begin with a comma", async () => {
    expect(await quickIds(db, ",abc,ghi")).toEqual(["id1", "id2"]);
    expect(await quickIds(db, ",ghi,abc")).toEqual(["id2", "id1"]);
  });
  it("can take a comma separated list of quick ids that ends with a comma", async () => {
    expect(await quickIds(db, "abc,ghi,")).toEqual(["id1", "id2"]);
    expect(await quickIds(db, "jkl,abc,")).toEqual(["id3", "id1"]);
  });
  it("can take an array of quick ids", async () => {
    expect(await quickIds(db, ["abc", "jkl"])).toEqual(["id1", "id3"]);
  });
  it("can take a string surrounded by [] that is interpreted as an array", async () => {
    expect(await quickIds(db, "[ghi,abc]")).toEqual(["id2", "id1"]);
  });

  it("still errors out if any one of the quick ids produces an error", async () => {
    await db.put({
      _id: "id4",
      meta: { humanId: "ghpo" },
    });
    try {
      await quickIds(db, ",jkl,gh");
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(AmbiguousQuickIdError);
    }
  });
});

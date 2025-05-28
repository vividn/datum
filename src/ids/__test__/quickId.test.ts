import {
  fail,
  testDbLifecycle,
  setNow,
  restoreNow,
} from "../../__test__/test-utils";
import { insertDatumView } from "../../views/insertDatumView";
import {
  stateChangeView,
  humanIdView,
  idToHumanView,
  subHumanIdView,
  timingView,
} from "../../views/datumViews";
import {
  AmbiguousQuickIdError,
  NoQuickIdMatchError,
  quickId,
} from "../quickId";
import { occurCmd } from "../../commands/occurCmd";
import { getCmd } from "../../commands/getCmd";
import { getLastDocs } from "../../documentControl/lastDocs";
import { toDatumTime } from "../../time/datumTime";

jest.retryTimes(3);

describe("quickId", () => {
  const dbName = "test_quick_id";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await insertDatumView({ db, datumView: idToHumanView });
    await insertDatumView({ db, datumView: subHumanIdView });
    await insertDatumView({ db, datumView: humanIdView });
    await insertDatumView({ db, datumView: stateChangeView });
  });

  test("it returns the string directly if the exact id exists in the database", async () => {
    await db.put({ _id: "exact-id", data: {}, meta: {} });
    const quick = await quickId("exact-id", {});
    expect(quick).toEqual(["exact-id"]);
  });

  test("if the text matches the beginning of exactly one humanId, it returns the associated _id", async () => {
    await db.put({ _id: "doc-id1", data: {}, meta: { humanId: "abcdefg" } });
    await db.put({ _id: "doc-id2", data: {}, meta: { humanId: "abzzzzz" } });

    const quick = await quickId("abc", {});
    expect(quick).toEqual(["doc-id1"]);
  });

  test("if the text matches more than one humanId, it throws an error, showing the possible matches. Or it follows the strategy of onAmbiguousQuickId", async () => {
    await db.put({ _id: "doc-id1", data: {}, meta: { humanId: "abzzzzz" } });
    await db.put({ _id: "doc-id2", data: {}, meta: { humanId: "abcdefg" } });

    try {
      await quickId("ab", {});
      fail();
    } catch (error) {
      expect(error).toBeInstanceOf(AmbiguousQuickIdError);
      const message = (error as AmbiguousQuickIdError).message;
      expect(message).toMatch(/\babc\b/);
      expect(message).toMatch(/\bdoc-id2\b/);
      expect(message).toMatch(/\babz\b/);
      expect(message).toMatch(/\bdoc-id1\b/);
    }

    const quickFirst = await quickId("ab", { onAmbiguousQuickId: "first" });
    expect(quickFirst).toEqual(["doc-id1"]); // gets first id, not first hid
    const quickLast = await quickId("ab", { onAmbiguousQuickId: "last" });
    expect(quickLast).toEqual(["doc-id2"]); // gets last id, not last hid
    const quickAll = await quickId("ab", { onAmbiguousQuickId: "all" });
    expect(quickAll).toEqual(expect.arrayContaining(["doc-id1", "doc-id2"]));
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

    const quick = await quickId("zzz", {});
    expect(quick).toEqual(["zzz_this_id"]);
  });

  test("if no humanIds match, but several _ids match starting sub string, throw error and show possible matches, or follow strategy of onAmbiguousQuickId", async () => {
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
      await quickId("zzz", {});
      fail();
    } catch (error) {
      expect(error).toBeInstanceOf(AmbiguousQuickIdError);
      const message = (error as AmbiguousQuickIdError).message;
      expect(message).toMatch(/\ba\b/);
      expect(message).toMatch(/\bzzz_this_id\b/);
      expect(message).toMatch(/\bd\b/);
      expect(message).toMatch(/\bzzz_same_start\b/);
    }

    const quickFirst = await quickId("zzz", { onAmbiguousQuickId: "first" });
    expect(quickFirst).toEqual(["zzz_same_start"]); // gets first id, not first hid
    const quickLast = await quickId("zzz", { onAmbiguousQuickId: "last" });
    expect(quickLast).toEqual(["zzz_this_id"]); // gets last id, not last hid
    const quickAll = await quickId("zzz", { onAmbiguousQuickId: "all" });
    expect(quickAll).toEqual(
      expect.arrayContaining(["zzz_this_id", "zzz_same_start"]),
    );
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

    const quick = await quickId("abc", {});
    expect(quick).toEqual(["another_id"]);
  });

  test("if the string matches an id exactly and a human id exactly, prefer the id match", async () => {
    await db.put({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });
    await db.put({
      _id: "human_is_like_other_id",
      data: {},
      meta: { humanId: "abcId" },
    });

    const quick = await quickId("abcId", {});
    expect(quick).toEqual(["abcId"]);
  });

  test("if the string matches an id exactly and is the beginning of one or more human ids, prefer the id", async () => {
    await db.put({ _id: "abcId", data: {}, meta: { humanId: "humanId" } });
    await db.put({
      _id: "human_starts_like_other_id",
      data: {},
      meta: { humanId: "abcId_extra" },
    });

    const quick = await quickId("abcId", {});
    expect(quick).toEqual(["abcId"]);

    await db.put({
      _id: "another_one_with_extra_human",
      data: {},
      meta: { humanId: "abcId_and_more" },
    });

    const quick2 = await quickId("abcId", {});
    expect(quick2).toEqual(["abcId"]);
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

    await expect(() => quickId("lmnop", {})).rejects.toThrow(
      NoQuickIdMatchError,
    );
  });

  test("it returns the lastdocs ref if the quickId is _LAST", async () => {
    const { _id: _id1 } = await occurCmd("firstField");
    const { _id: _id2 } = await occurCmd("secondField");
    // multi id lastDocsRef
    await getCmd(`,${_id1},${_id2}`);
    expect((await getLastDocs(db)).ids).toEqual([_id1, _id2]);
    const quick = await quickId("_LAST", {});
    expect(quick).toEqual([_id1, _id2]);
  });

  it("can take a comma separated list of quick ids", async () => {
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
    expect(await quickId(",abc,ghi", {})).toEqual(["id1", "id2"]);
    expect(await quickId(",ghi,abc", {})).toEqual(["id2", "id1"]);
    expect(await quickId("abc,ghi,", {})).toEqual(["id1", "id2"]);
    expect(await quickId("jkl,abc,", {})).toEqual(["id3", "id1"]);
    expect(await quickId(["abc", "jkl"], {})).toEqual(["id1", "id3"]);
  });

  it("still errors out if any one of the quick ids produces an error", async () => {
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
    await db.put({
      _id: "id4",
      meta: { humanId: "ghpo" },
    });
    try {
      await quickId(",jkl,gh", {});
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(AmbiguousQuickIdError);
    }
  });

  it("prefers an id match with commas over splitting by commas", async () => {
    await db.put({
      _id: "id1,ghi,and more",
      meta: { humanId: "abc" },
    });
    await db.put({
      _id: "id1",
      meta: { humanId: "ghi" },
    });
    await db.put({
      _id: "id2",
      meta: { humanId: "jkl" },
    });
    expect(await quickId("id1,ghi,and more", {})).toEqual(["id1,ghi,and more"]);
    expect(await quickId("id1,ghi", {})).toEqual(["id1,ghi,and more"]);
    expect(await quickId("id1,jkl", {})).toEqual(["id1", "id2"]);
  });
});

describe("quickId underscore notation", () => {
  const dbName = "test_underscore_shorthand";
  const db = testDbLifecycle(dbName);

  afterAll(() => {
    restoreNow();
  });

  beforeEach(async () => {
    await insertDatumView({ db, datumView: idToHumanView });
    await insertDatumView({ db, datumView: subHumanIdView });
    await insertDatumView({ db, datumView: humanIdView });
    await insertDatumView({ db, datumView: stateChangeView });
    await insertDatumView({ db, datumView: timingView });

    setNow("2025-05-28, 17:00");

    await db.put({
      _id: "sleep:1",
      data: { field: "sleep", value: "8h", occurTime: toDatumTime("16:00") },
      meta: {
        humanId: "sleep1",
        createTime: toDatumTime("16:00"),
      },
    });

    await db.put({
      _id: "sleep:2",
      data: { field: "sleep", value: "7h", occurTime: toDatumTime("16:30") },
      meta: {
        humanId: "sleep2",
        createTime: toDatumTime("16:30"),
      },
    });

    await db.put({
      _id: "alcohol:1",
      data: { field: "alcohol", type: "beer", occurTime: toDatumTime("16:10") },
      meta: {
        humanId: "alcohol1",
        createTime: toDatumTime("16:10"),
      },
    });

    await db.put({
      _id: "alcohol:2",
      data: { field: "alcohol", type: "wine", occurTime: toDatumTime("16:20") },
      meta: {
        humanId: "alcohol2",
        createTime: toDatumTime("16:20"),
      },
    });

    await db.put({
      _id: "alcohol:3",
      data: {
        field: "alcohol",
        type: "whiskey",
        occurTime: toDatumTime("16:40"),
      },
      meta: {
        humanId: "alcohol3",
        createTime: toDatumTime("16:40"),
      },
    });

    await db.put({
      _id: "note:1",
      data: {
        field: "note",
        text: "First note",
        occurTime: toDatumTime("16:50"),
      },
      meta: {
        humanId: "note1",
        createTime: toDatumTime("16:50"),
      },
    });
  });

  test("it returns the most recent document when using '_'", async () => {
    const quick = await quickId("_", {});
    expect(quick).toEqual(["note:1"]);
  });

  test("it returns the second most recent document when using '__' or '_2'", async () => {
    const quickDoubleUnderscore = await quickId("__", {});
    const quickNumbered = await quickId("_2", {});

    expect(quickDoubleUnderscore).toEqual(["alcohol:3"]);
    expect(quickNumbered).toEqual(["alcohol:3"]);
  });

  test("it returns the third most recent document when using '___' or '_3'", async () => {
    const quickTripleUnderscore = await quickId("___", {});
    const quickNumbered = await quickId("_3", {});

    expect(quickTripleUnderscore).toEqual(["sleep:2"]);
    expect(quickNumbered).toEqual(["sleep:2"]);
  });

  test("it returns field-specific recent documents when using '_fieldname'", async () => {
    const quickAlcohol = await quickId("_alcohol", {});
    expect(quickAlcohol).toEqual(["alcohol:3"]);

    const quickSleep = await quickId("_sleep", {});
    expect(quickSleep).toEqual(["sleep:2"]);
  });

  test("it returns field-specific nth recent document when using '__fieldname' or '_2:fieldname'", async () => {
    const quickAlcoholDouble = await quickId("__alcohol", {});
    const quickAlcoholNumbered = await quickId("_2:alcohol", {});

    expect(quickAlcoholDouble).toEqual(["alcohol:2"]);
    expect(quickAlcoholNumbered).toEqual(["alcohol:2"]);
  });

  test("it throws an error for non-existent positions", async () => {
    await expect(() => quickId("_10", {})).rejects.toThrow(NoQuickIdMatchError);
  });

  test("it throws an error for non-existent fields", async () => {
    await expect(() => quickId("_nonexistent", {})).rejects.toThrow(
      NoQuickIdMatchError,
    );
  });

  test("it handles very specific position requests correctly", async () => {
    const quickAlcoholThird = await quickId("_3:alcohol", {});
    expect(quickAlcoholThird).toEqual(["alcohol:1"]);
  });

  test("it fetches a document with only createTime (no occurTime)", async () => {
    await db.put({
      _id: "task:1",
      data: { field: "task", name: "createTime only" },
      meta: {
        humanId: "task1",
        createTime: toDatumTime("16:45"),
        // No occurTime provided
      },
    });

    // Should be the second most recent (after note:1 occurring at 16:50)
    const quickTask = await quickId("_task", {});
    expect(quickTask).toEqual(["task:1"]);

    const quickSecond = await quickId("_2", {});
    expect(quickSecond).toEqual(["task:1"]);
  });

  test("it prefers occurTime over createTime when fetching data by quickId _ notation", async () => {
    await db.put({
      _id: "meeting:1",
      data: {
        field: "meeting",
        topic: "Planning",
        occurTime: toDatumTime("18:00"),
      },
      meta: {
        humanId: "meeting1",
        createTime: toDatumTime("19:00"),
      },
    });

    await db.put({
      _id: "meeting:2",
      data: { field: "meeting", topic: "Planning" },
      meta: {
        humanId: "meeting2",
        createTime: toDatumTime("17:30"),
        // no occurTime
      },
    });

    await db.put({
      _id: "meeting:3",
      data: {
        field: "meeting",
        topic: "Planning",
        occurTime: toDatumTime("17:00"),
      },
      meta: {
        humanId: "meeting3",
        createTime: toDatumTime("20:00"),
      },
    });

    const quickMeeting = await quickId("_meeting", {});
    expect(quickMeeting).toEqual(["meeting:1"]);

    const quickPosition = await quickId("_3:meeting", {});
    expect(quickPosition).toEqual(["meeting:3"]);
  });
});

import { DatumPayload, EitherPayload } from "../DatumDocument";
import { DateTime } from "luxon";
import { fail, pass, resetTestDb, setNow } from "../../test-utils";
import {
  overwriteDoc,
  NoDocToOverwriteError,
  OverwriteDocError,
} from "../overwriteDoc";
import { jClone } from "../../utils/jClone";
import { addDoc } from "../addDoc";
import { DocExistsError } from "../base";

const testDatumPayload: DatumPayload = {
  data: {
    abc: 123,
    foo: "bar",
    occurTime: "2021-06-20T14:00:00Z",
    occurUtcOffset: 2,
  },
  meta: {
    random: 0.4869350234,
    idStructure: "%foo%__rawString",
    humanId: "ndke4ms9",
  },
};
const testDatumPayloadId = "bar__rawString";

const nowStr = "2021-06-20T18:45:00.000Z";
const notNowStr = "2010-11;12T13:14:15.000Z";

describe("overwriteDoc", () => {
  const dbName = "overwrite_doc_test";
  let db: PouchDB.Database<EitherPayload>;

  beforeEach(async () => {
    db = await resetTestDb(dbName);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  beforeEach(async () => {
    setNow(nowStr);
  });

  it("fails if id to be overwritten does not exist in db", async () => {
    await expect(
      overwriteDoc({ db, id: "does-not-exist", payload: { valid: "data" } })
    ).rejects.toThrowError(NoDocToOverwriteError);
    await expect(
      overwriteDoc({
        db,
        id: "does-not-exist",
        payload: { _id: "does-not-exist", data: "data" },
      })
    ).rejects.toThrowError(NoDocToOverwriteError);
    await expect(
      overwriteDoc({
        db,
        id: "does-not-exist",
        payload: { _id: "some-other-id", data: "data" },
      })
    ).rejects.toThrowError(NoDocToOverwriteError);
    await expect(
      overwriteDoc({
        db,
        id: "does-not-exist",
        payload: { data: { foo: "bar" }, meta: { idStructure: "%foo%" } },
      })
    ).rejects.toThrowError(NoDocToOverwriteError);
  });

  it("fails if new id clashes with a different document in the database", async () => {
    const oldId = "id-to-replace";
    const clashingId = "preexisting-clashing-id";
    await db.put({ _id: oldId });
    await db.put({ _id: clashingId });

    try {
      await overwriteDoc({
        db,
        id: oldId,
        payload: { _id: clashingId, foo: "bar" },
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    await db.get(oldId); // original doc is not deleted

    try {
      await overwriteDoc({
        db,
        id: oldId,
        payload: { _id: clashingId, data: {}, meta: {} },
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    await db.get(oldId);

    try {
      await overwriteDoc({
        db,
        id: oldId,
        payload: {
          data: { idField: clashingId },
          meta: { idStructure: "%idField%" },
        },
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    await db.get(oldId);
  });

  it("replaces the existing document if the new id is the same", async () => {
    await db.put({ _id: "existing-id", oldKey: "oldData" });

    const newDoc1 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: { _id: "existing-id", newKey1: "newData1" },
    });
    const dbDoc1 = await db.get("existing-id");
    expect(dbDoc1).toEqual(newDoc1);
    expect(newDoc1).toHaveProperty("newKey1", "newData1");
    expect(newDoc1).not.toHaveProperty("oldKey");

    const payload2 = {
      _id: "existing-id",
      data: { newKey2: "newData2" },
      meta: { humanId: "abcdefg" },
    };
    const newDoc2 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: payload2,
    });
    const dbDoc2 = await db.get("existing-id");
    expect(dbDoc2).toEqual(newDoc2);
    expect(newDoc2).toMatchObject(payload2);
    expect(newDoc2).not.toHaveProperty("newKey1");

    const payload3 = {
      data: { newKey3: "newData3", idKey: "existing-id" },
      meta: { idStructure: "%idKey%" },
    };
    const newDoc3 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: payload3,
    });
    const dbDoc3 = await db.get("existing-id");
    expect(dbDoc3).toEqual(newDoc3);
    expect(newDoc3).toMatchObject(payload3);
    expect(newDoc3).not.toHaveProperty("data.newKey2");
  });

  it("if new document does not have id, it replaces the doc at the old id", async () => {
    await db.put({ _id: "existing-id", oldKey: "oldData" });

    const newDoc1 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: { newKey1: "newData1" },
    });
    const dbDoc1 = await db.get("existing-id");
    expect(dbDoc1).toEqual(newDoc1);
    expect(newDoc1).toHaveProperty("newKey1", "newData1");
    expect(newDoc1).not.toHaveProperty("oldKey");

    const payload2 = {
      data: { newKey2: "newData2" },
      meta: { humanId: "abcdefg" },
    };
    const newDoc2 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: payload2,
    });
    const dbDoc2 = await db.get("existing-id");
    expect(dbDoc2).toEqual(newDoc2);
    expect(newDoc2).toMatchObject(payload2);
    expect(newDoc2).not.toHaveProperty("newKey1");
  });

  it("deletes the old document if the new document has a different id", async () => {
    await db.put({ _id: "old-id", oldKey: "oldData" });

    const payload1 = { _id: "new-id-1", newKey1: "newData1" };
    const newDoc1 = await overwriteDoc({ db, id: "old-id", payload: payload1 });
    await expect(db.get("old-id")).rejects.toThrowErrorMatchingInlineSnapshot(
      `"deleted"`
    );
    const dbDoc1 = await db.get("new-id-1");
    expect(dbDoc1).toEqual(newDoc1);
    expect(newDoc1).toMatchObject(payload1);
    expect(newDoc1).not.toHaveProperty("oldKey");

    const payload2 = {
      _id: "new-id-2",
      data: { newKey2: "newData2" },
      meta: { humanId: "abcdefgh" },
    };
    const newDoc2 = await overwriteDoc({
      db,
      id: "new-id-1",
      payload: payload2,
    });
    await expect(db.get("new-id-1")).rejects.toThrowErrorMatchingInlineSnapshot(
      `"deleted"`
    );
    const dbDoc2 = await db.get("new-id-2");
    expect(dbDoc2).toEqual(newDoc2);
    expect(newDoc2).toMatchObject(payload2);
    expect(newDoc2).not.toHaveProperty("newKey1");
    expect(newDoc2).not.toHaveProperty("data.newKey1");

    const payload3 = {
      data: { newKey3: "newData3", idKey: "new-id-3" },
      meta: { idStructure: "%idKey%" },
    };
    const newDoc3 = await overwriteDoc({
      db,
      id: "new-id-2",
      payload: payload3,
    });
    await expect(db.get("new-id-2")).rejects.toThrowErrorMatchingInlineSnapshot(
      `"deleted"`
    );
    const dbDoc3 = await db.get("new-id-3");
    expect(dbDoc3).toEqual(newDoc3);
    expect(newDoc3).toMatchObject(payload3);
    expect(newDoc3).not.toHaveProperty("data.newKey2");
  });

  it("updates modifyTime to now for DatumPayloads", async () => {
    const data1 = { foo: "bar" };
    const data2 = { bar: "baz" };
    const modMeta = { occurTime: notNowStr, modifyTime: notNowStr };
    const noModMeta = { occurTime: notNowStr };
    const modPay1 = { data: data1, meta: modMeta };
    const noModPay1 = { data: data1, meta: noModMeta };
    // These next two are used to avoid having equivalent payload and docs
    const modPay2 = { data: data2, meta: modMeta };
    const noModPay2 = { data: data2, meta: noModMeta };

    await db.put({ _id: "data-only-payload-1", ...data1 });
    const newDoc1 = await overwriteDoc({
      db,
      id: "data-only-payload-1",
      payload: noModPay1,
    });
    expect(newDoc1).toHaveProperty("meta.modifyTime", nowStr);

    await db.put({ _id: "data-only-payload-2", ...data1 });
    const newDoc2 = await overwriteDoc({
      db,
      id: "data-only-payload-2",
      payload: modPay1,
    });
    expect(newDoc2).toHaveProperty("meta.modifyTime", nowStr);

    await db.put({
      _id: "datum-without-modifyTime-1",
      ...noModPay1,
    });
    const newDoc3 = await overwriteDoc({
      db,
      id: "datum-without-modifyTime-1",
      payload: noModPay2,
    });
    expect(newDoc3).toHaveProperty("meta.modifyTime", nowStr);

    await db.put({
      _id: "datum-without-modifyTime-2",
      ...noModPay1,
    });
    const newDoc4 = await overwriteDoc({
      db,
      id: "datum-without-modifyTime-2",
      payload: modPay2,
    });
    expect(newDoc4).toHaveProperty("meta.modifyTime", nowStr);

    await db.put({ _id: "datum-with-modifyTime-1", ...modPay1 });
    const newDoc5 = await overwriteDoc({
      db,
      id: "datum-with-modifyTime-1",
      payload: noModPay2,
    });
    expect(newDoc5).toHaveProperty("meta.modifyTime", nowStr);

    await db.put({ _id: "datum-with-modifyTime-2", ...modPay1 });
    const newDoc6 = await overwriteDoc({
      db,
      id: "datum-with-modifyTime-2",
      payload: modPay2,
    });
    expect(newDoc6).toHaveProperty("meta.modifyTime", nowStr);
  });

  it("if metadata exists on both documents it uses the createTime of the old document, but otherwise all other metadata from the new document", async () => {
    const timeA = DateTime.utc(2010, 11, 12, 13, 14, 15).toString();
    const timeB = DateTime.utc(2013, 12, 11, 10, 9, 8).toString();
    const oldDoc = {
      _id: "doc-id",
      data: {},
      meta: {
        occurTime: timeA,
        createTime: timeA,
        humanId: "olddoc",
      },
    };
    const newPayload = {
      _id: "doc-id",
      data: {},
      meta: {
        occurTime: timeB,
        createTime: timeB,
        humanId: "newdoc",
      },
    };
    const expectedNewDoc = {
      _id: "doc-id",
      data: {},
      meta: {
        occurTime: timeB,
        createTime: timeA,
        humanId: "newdoc",
      },
    };
    await db.put(oldDoc);
    const newDoc = await overwriteDoc({
      db,
      id: "doc-id",
      payload: newPayload,
    });
    expect(newDoc).toMatchObject(expectedNewDoc);
  });

  it("if new doc is dataOnly, no metadata is saved from old doc", async () => {
    await db.put({
      _id: "document",
      data: {
        foo: "bar",
      },
      meta: {
        humanId: "abcdef",
      },
    });
    const newDoc = await overwriteDoc({
      db,
      id: "document",
      payload: { bar: "baz" },
    });
    expect(newDoc).not.toHaveProperty("meta");
    expect(newDoc).not.toHaveProperty("humanId");
  });

  it("if createTime or metadata does not exist on old document, new document does not have a createTime because it is unknown", async () => {
    await db.put({
      _id: "doc-without-meta",
      foo: "bar",
      occurTime: "2022-08-14T22:42:00Z",
      occurUtcOffset: 2,
    });
    await db.put({
      _id: "doc-without-createTime",
      data: {
        bar: "baz",
        occurTime: "2022-08-14T22:42:00Z",
        occurUtcOffset: 2,
      },
      meta: { humanId: "fedcba" },
    });
    const newPayload = {
      data: { foobar: "barbaz" },
      meta: { humanId: "abcdef" },
    };

    const newDoc1 = await overwriteDoc({
      db,
      id: "doc-without-meta",
      payload: newPayload,
    });
    const newDoc2 = await overwriteDoc({
      db,
      id: "doc-without-createTime",
      payload: newPayload,
    });

    expect(newDoc1).not.toHaveProperty("meta.createTime");
    expect(newDoc2).not.toHaveProperty("meta.createTime");
  });

  test("if payload specified a _rev, then it must match the _rev on the old document", async () => {
    await db.put({ _id: "abc", foo: "bar" });
    const oldDoc = await db.get("abc");
    const wrongRev = "1-38748349796ad6a60a11c0f63d10186a";

    await expect(
      overwriteDoc({
        db,
        id: "abc",
        payload: { _id: "abc", _rev: wrongRev, foo2: "bar2" },
      })
    ).rejects.toThrowError(OverwriteDocError);
    const stillOldDoc = await db.get("abc");
    expect(oldDoc).toEqual(stillOldDoc);

    await overwriteDoc({
      db,
      id: "abc",
      payload: { _id: "abc", _rev: oldDoc._rev, foo3: "bar3" },
    });

    const newDoc = await db.get("abc");
    expect(newDoc).toHaveProperty("foo3", "bar3");

    await expect(
      overwriteDoc({
        db,
        id: "abc",
        payload: { _rev: wrongRev, ...testDatumPayload },
      })
    ).rejects.toThrowError(OverwriteDocError);
    await overwriteDoc({
      db,
      id: "abc",
      payload: { _rev: newDoc._rev, ...testDatumPayload },
    });
    await expect(db.get("abc")).rejects.toThrowErrorMatchingInlineSnapshot(
      `"deleted"`
    );
  });

  it("does not alter the payload", async () => {
    await db.put({ _id: "abcd", foo: "bar" });
    const oldDoc = await db.get("abcd");
    const payload1 = {
      _id: "abcd",
      _rev: oldDoc._rev,
      data: {
        foo: "bar",
        abc: 123,
        occurTime: "2021-06-20T14:00:00Z",
        occurUtcOffset: 2,
      },
      meta: {},
    };
    const payload2 = jClone(payload1);
    expect(payload1).toEqual(payload2);

    await overwriteDoc({ db, id: "abcd", payload: payload1 });

    expect(payload1).toEqual(payload2);
  });

  it("does not write to db if payload is identical", async () => {
    await addDoc({ db, payload: testDatumPayload });
    const existingDoc = await db.get(testDatumPayloadId);

    const returnedDoc = await overwriteDoc({
      db,
      id: testDatumPayloadId,
      payload: testDatumPayload,
    });
    expect(returnedDoc).toEqual(existingDoc);
    expect((await db.get(testDatumPayloadId))._rev).toEqual(existingDoc._rev);
  });

  it("can overwrite a design document with a new one", async () => {
    const viewDoc1 = {
      _id: "_design/someView",
      views: {
        default: {
          map: "(doc) => {emit(doc._id, null);}",
        },
      },
    };
    const viewDoc2 = {
      _id: "_design/someView",
      views: {
        newViewName: {
          map: "(doc) => {emit(doc._id, null);}",
        },
      },
    };
    await db.put(viewDoc1);

    const newDoc = await overwriteDoc({
      db,
      id: "_design/someView",
      payload: viewDoc2,
    });
    expect(newDoc).toHaveProperty("views.newViewName.map");
    expect(newDoc).not.toHaveProperty("views.default");
  });

  it("updates the modifyTime of a design document", async () => {
    const viewDoc1 = {
      _id: "_design/someView",
      views: {
        default: {
          map: "(doc) => {emit(doc._id, null);}",
        },
      },
      meta: { modifyTime: notNowStr },
    };
    const viewDoc2 = {
      _id: "_design/someView",
      views: {
        newViewName: {
          map: "(doc) => {emit(doc._id, null);}",
        },
      },
      meta: {},
    };
    await db.put(viewDoc1);

    const newDoc = await overwriteDoc({
      db,
      id: "_design/someView",
      payload: viewDoc2,
    });
    expect(newDoc).toHaveProperty("meta.modifyTime", nowStr);
  });

  it("does not write to db if view is identical", async () => {
    const viewDoc1 = {
      _id: "_design/someView",
      views: {
        default: {
          map: "(doc) => {emit(doc._id, null);}",
        },
      },
      meta: { modifyTime: notNowStr },
    };
    await db.put(viewDoc1);
    const dbDoc = await db.get("_design/someView");

    const newDoc = await overwriteDoc({
      db,
      id: "_design/someView",
      payload: viewDoc1,
    });
    expect(newDoc._rev).toEqual(dbDoc._rev);
  });
});

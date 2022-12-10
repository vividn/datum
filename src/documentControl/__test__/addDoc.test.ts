import {
  DataOnlyDocument,
  DataOnlyPayload,
  DatumDocument,
  DatumPayload,
} from "../DatumDocument";
import { fail, setNow, testDbLifecycle } from "../../test-utils";
import { addDoc } from "../addDoc";
import { IdError } from "../../errors";
import { jClone } from "../../utils/jClone";
import * as updateDoc from "../updateDoc";
import * as overwriteDoc from "../overwriteDoc";
import * as deleteDoc from "../deleteDoc";
import { DocExistsError } from "../base";
import { _emit as emit } from "../../views/emit";
import { Show } from "../../input/outputArgs";

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

describe("addDoc", () => {
  const dbName = "add_doc_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    setNow(nowStr);
  });

  it("it adds dataOnly payloads with _id to the given database", async () => {
    const id = "dataOnlyPayloadId";
    const payload = { _id: id, a: 1, c: "dataString" };

    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(id)) as DataOnlyDocument;
    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(payload);
  });

  it("throws error if trying to add dataOnly payload without _id", async () => {
    const payload = { a: 1, c: "dataString" };
    await expect(addDoc({ db, payload })).rejects.toThrowError(IdError);
  });

  it("adds a datum payload to db with _id if there is no idStructure", async () => {
    const payload = {
      _id: "DatumData_with_no_structure",
      data: { abc: "abc" },
      meta: { humanId: "abc" },
    } as DatumPayload;

    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(
      "DatumData_with_no_structure"
    )) as DatumDocument;
    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(payload);
  });

  it("adds a datum payload to the database with its calculated id", async () => {
    const payload = testDatumPayload;
    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(testDatumPayloadId)) as DatumDocument;

    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(testDatumPayload);
  });

  it("prefers to use the id calculated from structure", async () => {
    const payload = testDatumPayload;

    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(testDatumPayloadId)) as DatumDocument;
    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(testDatumPayload);
  });

  it("throws if datumPayload has neither id nor idStructure", async () => {
    const payload = {
      data: { abc: "123" },
      meta: { humanId: "ndke4ms9" },
    } as DatumPayload;
    await expect(addDoc({ db, payload })).rejects.toThrowError(IdError);
  });

  it("adds createTime and modifyTime to metadata of datumPayload", async () => {
    const payload = testDatumPayload;
    const newDoc = await addDoc({ db, payload });

    expect(newDoc.meta).toMatchObject({
      createTime: nowStr,
      modifyTime: nowStr,
    });
  });

  it("does not write to db if identical DataOnlyDocument exists", async () => {
    const id = "existingId";
    const existingData = { _id: id, abc: 123 } as DataOnlyPayload;
    await db.insert(existingData);
    const existingDoc = await db.get(id);

    const newDoc = await addDoc({ db, payload: existingData });
    expect(newDoc).toEqual(existingDoc);
    expect(await db.get(id)).toHaveProperty("_rev", existingDoc._rev);
  });

  it("does not write to db if DatumDocument with identical data already exists", async () => {
    const id = "existingDatumId";
    const existingPayload = {
      _id: id,
      data: { foo: "abc" },
      meta: { humanId: "meta can be different" },
    };
    const payloadSameData = {
      _id: id,
      data: { foo: "abc" },
      meta: { humanId: "different metadata" },
    };
    await db.insert(existingPayload);
    const existingDoc = await db.get(id);

    const newDoc = await addDoc({ db, payload: payloadSameData });
    expect(newDoc).toEqual(existingDoc);
    expect(await db.get(id)).toHaveProperty("_rev", existingDoc._rev);
    expect(newDoc.meta.humanId).toEqual("meta can be different");
  });

  it("does not write to db if ViewDocument with identical views already exists", async () => {
    const testViews = {
      viewName: {
        map: ((_doc: any) => {
          emit(null, null);
        }).toString(),
      },
    };
    const id = "_design/viewName";
    await db.insert({ _id: id, views: testViews, meta: {} });
    const existingDoc = await db.get(id);

    const newDoc = await addDoc({
      db,
      payload: { _id: id, views: testViews, meta: {} },
    });
    expect(newDoc).toEqual(existingDoc);
  });

  it("throws error if a different document with id already exists", async () => {
    const id = "existingId";
    const existingData = { _id: id, abc: 123 } as DataOnlyPayload;
    await db.insert(existingData);

    const attemptedNewPayload = {
      _id: id,
      newData: "but this won't get inserted",
    };

    try {
      await addDoc({ db, payload: attemptedNewPayload });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    const dbDoc = await db.get(id);
    expect(dbDoc).toMatchObject(existingData);
    expect(dbDoc).not.toHaveProperty("newData");
  });

  it("it still fails if data is the same, but payload is datum and the existing is dataonly", async () => {
    const data = { foo: "abc" };
    await db.insert({ _id: "dataOnly", ...data });

    try {
      await addDoc({
        db,
        payload: {
          _id: "dataOnly",
          data: { ...data },
          meta: { humanId: "human" },
        },
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
  });

  it("it still fails if data is the same, but payload is dataOnly and the existing is datum", async () => {
    const data = { _id: "datum", foo: "abc" };
    await db.insert({
      _id: "datum",
      data: { ...data },
      meta: { humanId: "datumDoc" },
    });
    try {
      await addDoc({ db, payload: { ...data } });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
  });

  it("still calls updateDoc with updateStrategy is even if data is identical", async () => {
    const spy = jest.spyOn(updateDoc, "updateDoc");
    const data = { _id: "dataonly", foo: "abc" };
    await db.insert(data);

    const newDoc = await addDoc({ db, payload: data, conflictStrategy: "xor" });
    expect(spy).toHaveBeenCalled();
    // xor removes properties that appear in both
    expect(newDoc).not.toHaveProperty("foo");
  });

  it("can still insert if _rev is given in the payload", async () => {
    const rev = "1-974bb250edb4c7da3b2f6459b2411873";
    const newDoc = await addDoc({
      db,
      payload: { ...testDatumPayload, _rev: rev },
    });
    expect(newDoc).toMatchObject(testDatumPayload);
    expect(newDoc._rev).not.toEqual(rev);
  });

  it("does not alter the payload", async () => {
    const payload1 = {
      _id: "abcd",
      _rev: "1-974bb250edb4c7da3b2f6459b2411873",
      data: {
        foo: "bar",
        abc: 123,
      },
      meta: {
        occurTime: "2021-06-20T14:00:00Z",
      },
    };
    const payload2 = jClone(payload1);
    expect(payload1).toEqual(payload2);

    await addDoc({ db, payload: payload1 });

    expect(payload1).toEqual(payload2);
  });

  it("calls updateDoc if id already exists and conflict strategy is given", async () => {
    const originalPayload = { _id: "docId", foo: "bar", anotherKey: "data" };
    const updatePayload = { _id: "docId", foo: "baz" };
    const conflictStrategy = "merge";
    const expectedResult = {
      _id: "docId",
      foo: ["bar", "baz"],
      anotherKey: "data",
    };
    const spy = jest.spyOn(updateDoc, "updateDoc");

    await addDoc({ db, payload: originalPayload });
    const newDoc = await addDoc({
      db,
      payload: updatePayload,
      conflictStrategy: conflictStrategy,
    });

    expect(newDoc).toMatchObject(expectedResult);
    expect(spy).toHaveBeenCalled();
  });

  it("calls overwriteDoc if conflict and 'overwrite' given as conflict strategy", async () => {
    const originalPayload: DatumPayload = {
      _id: "docId",
      data: { foo: "bar", anotherKey: "data" },
      meta: { humanId: "original" },
    };
    const overwritePayload: DatumPayload = {
      _id: "docId",
      data: { foo: "baz" },
      meta: { humanId: "overwrite" },
    };
    const conflictStrategy = "overwrite";
    const expectedResult: DatumPayload = {
      _id: "docId",
      data: { foo: "baz" },
      meta: { humanId: "overwrite" },
    };
    const overwriteSpy = jest.spyOn(overwriteDoc, "overwriteDoc");

    await addDoc({ db, payload: originalPayload });
    const newDoc = await addDoc({
      db,
      payload: overwritePayload,
      conflictStrategy,
    });
    expect(newDoc).toMatchObject(expectedResult);
    expect(overwriteSpy).toHaveBeenCalled();
  });

  it("calls deleteDoc if conflict and 'delete' given as conflict strategy", async () => {
    const originalPayload: DatumPayload = {
      _id: "docId",
      data: { foo: "bar", anotherKey: "data" },
      meta: { humanId: "original" },
    };
    const deletePayload: DatumPayload = {
      _id: "docId",
      data: { none: "of_this", data: "matters" },
      meta: { humanId: "delete" },
    };
    const conflictStrategy = "delete";
    const expectedResult = {
      _id: "docId",
      _deleted: true,
    };

    const deleteSpy = jest.spyOn(deleteDoc, "deleteDoc");

    await addDoc({ db, payload: originalPayload });
    const newDoc = await addDoc({
      db,
      payload: deletePayload,
      conflictStrategy,
    });
    expect(newDoc).toMatchObject(expectedResult);
    expect(deleteSpy).toHaveBeenCalled();
  });

  it("does not call other documentControl strategies if there is no conflict", async () => {
    const updateSpy = jest.spyOn(updateDoc, "updateDoc");
    const overwriteSpy = jest.spyOn(overwriteDoc, "overwriteDoc");
    const deleteSpy = jest.spyOn(deleteDoc, "deleteDoc");

    await addDoc({ db, payload: { _id: "new_id", data: {}, meta: {} } });
    await addDoc({ db, payload: { _id: "data_only_new", foo: "bar" } });

    expect(updateSpy).not.toHaveBeenCalled();
    expect(overwriteSpy).not.toHaveBeenCalled();
    expect(deleteSpy).not.toHaveBeenCalled();
  });

  test("It still throws an DocExistsError if conflict and showOutput", async () => {
    const originalLog = console.log;
    console.log = jest.fn();

    const id = "conflictId";
    await db.insert({ _id: id, foo: "abc" });
    try {
      await addDoc({
        db,
        payload: { _id: id, foo: "different" },
        show: Show.Standard,
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }

    console.log = originalLog;
  });

  test("it can do an id with %?createTime% as id_structure", async () => {
    await expect(() => db.get(nowStr)).rejects.toThrow("missing");
    const newDoc = await addDoc({
      db,
      payload: { data: {}, meta: { idStructure: "%?createTime%" } },
    });
    expect(newDoc._id).toEqual(nowStr);
  });

  test("it can do an id with %?modifyTime% as id_structure", async () => {
    await expect(() => db.get(nowStr)).rejects.toThrow("missing");
    const newDoc = await addDoc({
      db,
      payload: { data: {}, meta: { idStructure: "%?modifyTime%" } },
    });
    expect(newDoc._id).toEqual(nowStr);
  });

  test("it can add a design document to the database", async () => {
    const designPayload = {
      _id: "_design/viewDoc",
      views: {
        default: {
          map: "(doc) => {emit(doc._id, null);}",
        },
      },
    };

    const newDoc = await addDoc({ db, payload: designPayload });
    expect(newDoc).toHaveProperty("views.default.map");
  });

  test("it adds createTime and modifyTime to a design document with a meta field in the payload", async () => {
    const designPayload = {
      _id: "_design/viewDoc",
      views: {
        default: {
          map: "(doc) => {emit(doc._id, null);}",
        },
      },
      meta: {},
    };

    const newDoc = await addDoc({ db, payload: designPayload });
    expect(newDoc).toHaveProperty("meta.createTime", nowStr);
    expect(newDoc).toHaveProperty("meta.modifyTime", nowStr);
  });
});

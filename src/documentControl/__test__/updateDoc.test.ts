import { DatumDocument, DatumMetadata, DatumPayload } from "../DatumDocument";
import { fail, setNow, testDbLifecycle } from "../../__test__/test-utils";
import { updateDoc, NoDocToUpdateError, UpdateDocError } from "../updateDoc";
import { addDoc } from "../addDoc";
import * as combineData from "../combineData";
import { jClone } from "../../utils/jClone";
import { DocExistsError } from "../base";
import { DatumView, ViewDocument } from "../../views/DatumView";
import { insertDatumView } from "../../views/insertDatumView";
import { datumViewToViewPayload } from "../../views/datumViewToViewPayload";
import { toDatumTime } from "../../time/datumTime";

const testDatumPayload: DatumPayload = {
  data: {
    abc: 123,
    foo: "bar",
    occurTime: {
      utc: "2021-06-20T14:00:00Z",
      o: 2,
      tz: "Europe/Berlin",
    },
  },
  meta: {
    idStructure: "%foo%__rawString",
    humanId: "ndke4ms9",
  },
};

const testDatumPayloadId = "bar__rawString";
const nowStr = "2021-06-20T18:45:00.000Z";
const now = toDatumTime(nowStr);
const notNowStr = "2010-11-12T13:14:15.000Z";
const notNow = toDatumTime(notNowStr);

describe("updateDoc", () => {
  const dbName = "update_doc_test";
  const db = testDbLifecycle(dbName);

  test("it returns the updated document in the db", async () => {
    await db.put({ _id: "docId1", data: { abc: "123" }, meta: {} });
    const returnedDoc1 = await updateDoc({
      db,
      id: "docId1",
      payload: {
        newKey: "newData",
      },
    });
    const dbDoc1 = await db.get("docId1");
    expect(returnedDoc1).toEqual(dbDoc1);

    await db.put({ _id: "docId2", def: "456" });
    const returnedDoc2 = await updateDoc({
      db,
      id: "docId2",
      payload: { newKey: "newData" },
    });
    const dbDoc2 = await db.get("docId2");
    expect(returnedDoc2).toEqual(dbDoc2);
  });

  test("updates modifyTime if oldDoc has metadata", async () => {
    setNow(nowStr);

    const docWithModify = {
      _id: "doc-to-update",
      data: { abc: "def" },
      meta: { modifyTime: notNowStr },
    };
    await db.put(docWithModify);

    const newDoc = await updateDoc({
      db,
      id: "doc-to-update",
      payload: {
        newKey: "newData",
      },
    });

    expect(newDoc).toHaveProperty("meta.modifyTime.utc", nowStr);
  });

  test("does not add metadata if oldDoc does not have metadata", async () => {
    await db.put({ _id: "docWithoutMeta", key: "data" });
    const newDoc = await updateDoc({
      db,
      id: "docWithoutMeta",
      payload: testDatumPayload,
    });
    expect(newDoc).not.toHaveProperty("meta");
  });

  test("keeps all the metadata in oldDoc (except modifyTime), and does not add anything from the payload", async () => {
    const oldDoc = (await addDoc({
      db,
      payload: testDatumPayload,
    })) as DatumDocument;
    const newPayload = {
      data: {
        newKey: "newData",
        occurTime: {
          utc: "2021-08-09T14:13:00Z",
          o: 1,
          tz: "Europe/Lisbon",
        },
      },
      meta: {
        idStructure: "%newKey%",
        random: 0.666,
        humanId: "noneOfThisShouldBeIncluded",
      },
    } as DatumPayload;
    const newDoc = (await updateDoc({
      db,
      id: testDatumPayloadId,
      payload: newPayload,
    })) as DatumDocument;

    const oldDocMeta = { ...oldDoc.meta };
    delete oldDocMeta.modifyTime;
    expect(newDoc.meta).toMatchObject(oldDocMeta);

    await expect(db.get("newData")).rejects.toMatchObject({
      name: "not_found",
      reason: "missing",
    });
  });

  test("different combination of dataOnly and datum for oldDoc and payload call the combineData function with appropriate arguments data component", async () => {
    const data1 = { abc: 123 };
    const data2 = { def: 456 };
    const data3 = { two: "fields", andTwo: "data" };
    const data4 = { another: null, set: "ofData" };
    const metadata = { ...testDatumPayload.meta } as DatumMetadata;
    delete metadata.idStructure;

    const spy = jest.spyOn(combineData, "combineData");

    await db.put({ _id: "data-doc-1", ...data1 });
    await updateDoc({ db, id: "data-doc-1", payload: data2 });
    expect(spy).toHaveBeenCalledWith(
      { _id: "data-doc-1", ...data1 },
      data2,
      "update",
    );
    spy.mockClear();

    await db.put({ _id: "data-doc-2", ...data3 });
    const datumPayload2 = { data: data4, meta: metadata } as DatumPayload;
    await updateDoc({
      db,
      id: "data-doc-2",
      payload: datumPayload2,
      updateStrategy: "removeConflicting",
    });
    expect(spy).toHaveBeenCalledWith(
      { _id: "data-doc-2", ...data3 },
      data4,
      "removeConflicting",
    );
    spy.mockClear();

    await db.put({ _id: "datum-doc-3", data: data2, meta: metadata });
    await updateDoc({
      db,
      id: "datum-doc-3",
      payload: data3,
      updateStrategy: "intersection",
    });
    expect(spy).toHaveBeenCalledWith(data2, data3, "intersection");
    spy.mockClear();

    await db.put({ _id: "datum-doc-4", data: data4, meta: metadata });
    await updateDoc({
      db,
      id: "datum-doc-4",
      payload: { data: data1, meta: metadata },
      updateStrategy: "append",
    });
    expect(spy).toHaveBeenCalledWith(data4, data1, "append");
  });

  test("fails if id to be updated does not exist in db", async () => {
    await expect(
      updateDoc({ db, id: "does-not-exist", payload: { valid: "data" } }),
    ).rejects.toThrow(NoDocToUpdateError);
  });

  test("fails if new id clashes with a different document in the database", async () => {
    const oldId = "id-to-replace";
    const clashingId = "preexisting-clashing-id";
    await db.put({ _id: oldId });
    await db.put({ _id: clashingId });

    try {
      await updateDoc({
        db,
        id: oldId,
        payload: { _id: clashingId, foo: "bar" },
        updateStrategy: "useNew",
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    await db.get(oldId); // original doc is not deleted

    const oldCalculatedId = "old-calc-id";
    await db.put({
      _id: oldCalculatedId,
      data: { foo: oldCalculatedId },
      meta: { idStructure: "%foo%" },
    });
    try {
      await updateDoc({
        db,
        id: oldCalculatedId,
        payload: {
          foo: clashingId,
        },
        updateStrategy: "useNew",
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    await db.get(oldCalculatedId);
  });

  test("if updated dataonly payload does not have id, then it uses the old id", async () => {
    await db.put({ _id: "data-id", dataKey: "abc" });
    const newDoc = await updateDoc({
      db,
      id: "data-id",
      payload: { newKey: "newData" },
      updateStrategy: "useNew",
    });
    expect(newDoc._id).toEqual("data-id");
    expect(newDoc).not.toHaveProperty("dataKey");
  });

  test("if the new document has a different explicit id, then the doc is moved there", async () => {
    await db.put({ _id: "old-id", foo: "bar" });
    const newDoc = await updateDoc({
      db,
      id: "old-id",
      payload: { _id: "new-id", other: "data" },
      updateStrategy: "update",
    });
    await expect(db.get("old-id")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
    expect(await db.get("new-id")).toEqual(newDoc);
    expect(newDoc).toMatchObject({ _id: "new-id", foo: "bar", other: "data" });
  });

  test("if the new document has a different calculated id, then the doc is moved there", async () => {
    await db.put({
      _id: "calculated-id",
      data: { foo: "calculated-id" },
      meta: { idStructure: "%foo%" },
    });
    const newDoc = await updateDoc({
      db,
      id: "calculated-id",
      payload: { foo: "new-calculated-id" },
      updateStrategy: "update",
    });
    await expect(db.get("calculated-id")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
    expect(newDoc._id).toEqual("new-calculated-id");
    expect(await db.get("new-calculated-id")).toEqual(newDoc);
    expect(newDoc).toHaveProperty("data.foo", "new-calculated-id");
  });

  test("if payload specified a _rev, then it must match the _rev on the old document", async () => {
    await db.put({ _id: "abc", foo: "bar" });
    const oldDoc = await db.get("abc");
    const wrongRev = "1-38748349796ad6a60a11c0f63d10186a";

    await expect(
      updateDoc({
        db,
        id: "abc",
        payload: { _id: "abc", _rev: wrongRev, foo2: "bar2" },
        updateStrategy: "useNew",
      }),
    ).rejects.toThrow(UpdateDocError);
    const stillOldDoc = await db.get("abc");
    expect(oldDoc).toEqual(stillOldDoc);

    await updateDoc({
      db,
      id: "abc",
      payload: { _id: "abc", _rev: oldDoc._rev, foo3: "bar3" },
      updateStrategy: "useNew",
    });

    const newDoc = await db.get("abc");
    expect(newDoc).toHaveProperty("foo3", "bar3");

    await expect(
      updateDoc({
        db,
        id: "abc",
        payload: { _rev: wrongRev, ...testDatumPayload },
        updateStrategy: "useNew",
      }),
    ).rejects.toThrow(UpdateDocError);
    await updateDoc({
      db,
      id: "abc",
      payload: { _rev: newDoc._rev, ...testDatumPayload },
      updateStrategy: "useNew",
    });
  });

  test("it does not modify the input payload", async () => {
    await db.put({ _id: "ididid", abc: 123 });
    const payload = { key1: "data1", key2: "data2" };
    const payloadClone = jClone(payload);

    await updateDoc({ db, id: "ididid", payload: payload });
    expect(payload).toEqual(payloadClone);
  });

  test("it successfully merges new data into the existing data", async () => {
    await db.put({
      _id: "doc-id",
      oldKey: "oldData",
      mutualKey: ["merge", "basis"],
      anotherMutual: "fromOld",
    });
    const newDoc = await updateDoc({
      db,
      id: "doc-id",
      payload: {
        newKey: "newData",
        mutualKey: ["will", "merge", "without", "duplicates"],
        anotherMutual: null,
      },
      updateStrategy: "merge",
    });
    expect(newDoc).toMatchObject({
      _id: "doc-id",
      oldKey: "oldData",
      newKey: "newData",
      mutualKey: ["merge", "basis", "will", "without", "duplicates"],
      anotherMutual: ["fromOld", null],
    });
  });

  test.todo("how does it handle _ids for dataonly docs?");

  test("it updates fieldStructure in metadata when updating composite field", async () => {
    // Create a document with a simple field
    await db.put({
      _id: "simple-field-doc",
      data: { field: "simple", something: "else" },
      meta: {},
    });

    // Update with a composite field
    const newDoc = await updateDoc({
      db,
      id: "simple-field-doc",
      payload: { field: "composite_%something%" },
    });

    // Check field is interpolated and fieldStructure is stored
    expect(newDoc).toHaveProperty("data.field", "composite_else");
    expect(newDoc).toHaveProperty(
      "meta.fieldStructure",
      "composite_%something%",
    );

    // Update again with a different composite field
    const updatedDoc = await updateDoc({
      db,
      id: "simple-field-doc",
      payload: { field: "%something%_updated" },
    });

    // Check field structure is updated
    expect(updatedDoc).toHaveProperty("data.field", "else_updated");
    expect(updatedDoc).toHaveProperty(
      "meta.fieldStructure",
      "%something%_updated",
    );

    // Update with a simple field (not a template)
    const finalDoc = (await updateDoc({
      db,
      id: "simple-field-doc",
      payload: { field: "back-to-simple" },
    })) as DatumDocument;

    // Check that fieldStructure is removed
    expect(finalDoc).toHaveProperty("data.field", "back-to-simple");
    // Check meta exists before accessing fieldStructure
    expect(finalDoc).toHaveProperty("meta");
    expect(finalDoc.meta?.fieldStructure).toBeUndefined();
  });

  test("it does not write to database if updated data document is identical", async () => {
    await db.put({ _id: "datadoc-id", foo: "abc" });
    const currentDoc = await db.get("datadoc-id");
    const nDocsBefore = (await db.info()).doc_count;

    const newDoc = await updateDoc({
      db,
      id: "datadoc-id",
      payload: { foo: "abc" },
      updateStrategy: "useNew",
    });
    expect(newDoc._rev).toEqual(currentDoc._rev);
    const nDocsAfter = (await db.info()).doc_count;
    expect(nDocsBefore).toEqual(nDocsAfter);
  });

  test("it does not write to db if updated datum document is identical", async () => {
    // Datum
    await db.put({
      _id: "datum-id",
      data: { abc: "foo" },
      meta: { humanId: "datumDatum" },
    });
    const currentDDoc = await db.get("datum-id");
    const nDocsBefore2 = (await db.info()).doc_count;

    const newDDoc = await updateDoc({
      db,
      id: "datum-id",
      payload: {
        data: { keys: "that", will: "be ignored" },
        meta: { random: 0.010101 },
      },
      updateStrategy: "useOld",
    });
    expect(newDDoc._rev).toEqual(currentDDoc._rev);
    const nDocsAfter2 = (await db.info()).doc_count;
    expect(nDocsBefore2).toEqual(nDocsAfter2);
  });

  test("it can update the id with new modifyTime when idStructure is %?modifyTime%", async () => {
    await expect(() => db.get(nowStr)).rejects.toMatchObject({
      name: "not_found",
      reason: "missing",
    });
    await db.put({
      _id: notNowStr,
      data: {},
      meta: { modifyTime: notNowStr, idStructure: "%?modifyTime%" },
    });
    const newDoc = await updateDoc({
      db,
      id: notNowStr,
      payload: { foo: "bar" },
    });
    expect(newDoc._id).toEqual(nowStr);
    await expect(() => db.get(notNowStr)).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
  });

  test("it properly handles field and id changes when field is part of the id", async () => {
    // Create a document with a field that's used in the id
    await db.put({
      _id: "original_field:test_id",
      data: {
        field: "original_field",
        category: "test",
      },
      meta: {
        idStructure: "%field%:%category%_id",
      },
    });

    // Update with a composite field
    const newDoc = await updateDoc({
      db,
      id: "original_field:test_id",
      payload: {
        field: "%category%_field",
      },
    });

    // Check that field is interpolated, fieldStructure is stored, and ID is updated
    expect(newDoc).toHaveProperty("data.field", "test_field");
    expect(newDoc).toHaveProperty("meta.fieldStructure", "%category%_field");
    expect(newDoc._id).toEqual("test_field:test_id");

    // Verify old document is removed
    await expect(db.get("original_field:test_id")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });

    // Update again changing both field and a value that affects the field interpolation
    const updatedDoc = await updateDoc({
      db,
      id: "test_field:test_id",
      payload: {
        field: "%category%_updated",
        category: "new",
      },
    });

    // Check that everything is updated correctly
    expect(updatedDoc).toHaveProperty("data.field", "new_updated");
    expect(updatedDoc).toHaveProperty(
      "meta.fieldStructure",
      "%category%_updated",
    );
    expect(updatedDoc._id).toEqual("new_updated:new_id");

    // Verify previous document is removed
    await expect(db.get("test_field:test_id")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
  });

  describe("update view documents", () => {
    function emit(_key: any, _value: any) {
      // pass
    }
    const view1: DatumView = {
      name: "test_view",
      map: (_doc) => {
        emit(1, 1);
      },
    };
    const view1Payload = datumViewToViewPayload(view1);
    const view2: DatumView = {
      name: "test_view",
      map: (_doc) => {
        emit(2, 2);
      },
    };
    const view2Payload = datumViewToViewPayload(view2);

    test("it can update view docuements", async () => {
      const viewDoc = await insertDatumView({
        db,
        datumView: view1,
      });
      expect(viewDoc._id).toEqual("_design/test_view");

      const updatedViewDoc = (await updateDoc({
        db,
        id: viewDoc._id,
        payload: view2Payload,
      })) as ViewDocument;
      expect(updatedViewDoc._id).toEqual("_design/test_view");
      expect(updatedViewDoc._rev).not.toEqual(viewDoc._rev);

      expect(updatedViewDoc.views.test_view.map.toString()).toContain(
        "emit(2, 2)",
      );
    });

    test("it updates the modify time of view documents", async () => {
      setNow(notNowStr);
      const viewDoc = await insertDatumView({
        db,
        datumView: view1,
      });
      expect(viewDoc.meta?.modifyTime).toEqual(notNow);

      setNow(nowStr);
      const updatedViewDoc = (await updateDoc({
        db,
        id: viewDoc._id,
        payload: view2Payload,
      })) as ViewDocument;
      expect(updatedViewDoc.meta?.modifyTime).toEqual(now);
    });

    test("it does not update view documents if they are identical", async () => {
      const viewDoc = await insertDatumView({
        db,
        datumView: view1,
      });
      const updatedViewDoc = (await updateDoc({
        db,
        id: viewDoc._id,
        payload: view1Payload,
      })) as ViewDocument;
      expect(updatedViewDoc._rev).toEqual(viewDoc._rev);
    });

    test("it does not update view documents if useOld is the strategy", async () => {
      const viewDoc = await insertDatumView({
        db,
        datumView: view1,
      });
      const updatedViewDoc = (await updateDoc({
        db,
        id: viewDoc._id,
        payload: view2Payload,
        updateStrategy: "useOld",
      })) as ViewDocument;
      expect(updatedViewDoc._rev).toEqual(viewDoc._rev);
    });

    test("it throws an error for updating view documents with unsupported strategies", async () => {
      const viewDoc = await insertDatumView({
        db,
        datumView: view1,
      });
      await expect(
        updateDoc({
          db,
          id: viewDoc._id,
          payload: view2Payload,
          updateStrategy: "merge",
        }),
      ).rejects.toThrow(UpdateDocError);
    });
  });
});

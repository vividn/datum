import { updateDoc } from "../updateDoc";
import * as updateDocModule from "../updateDoc";
import { fail, mockedLogLifecycle, testDbLifecycle } from "../../test-utils";
import { addDoc } from "../addDoc";
import { DocExistsError } from "../base";
import { overwriteDoc } from "../overwriteDoc";
import * as addDocModule from "../addDoc";
import { addCmd } from "../../commands/addCmd";
import { deleteDoc } from "../deleteDoc";
import { Show } from "../../input/outputArgs";

describe("output", () => {
  const dbName = "doc_control_output_test";
  const db = testDbLifecycle(dbName);
  const mockedLog = mockedLogLifecycle();

  test("addDoc displays a CREATE: message and the document if showOutput", async () => {
    await addDoc({
      db,
      payload: { _id: "added-doc", abc: "def" },
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
  });

  test("addDoc displays an EXISTS: and FAILED: message if showOuput and conlfict", async () => {
    await db.put({ _id: "alreadyHere", foo: "bar" });
    try {
      await addDoc({
        db,
        payload: { _id: "alreadyHere", baz: "bazzy" },
        outputArgs: {
          show: Show.Standard,
        },
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }

    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
  });

  test("addDoc calls updateDoc with showOutput", async () => {
    const updateDocSpy = jest.spyOn(updateDocModule, "updateDoc");
    const payload = { _id: "docId", foo: "abce" };
    await addDoc({
      db,
      payload,
      outputArgs: {
        show: Show.Standard,
      },
      conflictStrategy: "merge",
    });
    expect(updateDocSpy).not.toHaveBeenCalled();

    mockedLog.mockClear();

    await addDoc({
      db,
      payload,
      outputArgs: {
        show: Show.Standard,
      },
      conflictStrategy: "merge",
    });
    expect(updateDocSpy.mock.calls[0][0].outputArgs?.show).toEqual(
      Show.Standard
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));

    mockedLog.mockClear();

    await addDoc({
      db,
      payload: { ...payload, extraKey: 123 },
      outputArgs: {
        show: Show.Standard,
      },
      conflictStrategy: "merge",
    });
    expect(updateDocSpy.mock.calls[1][0].outputArgs?.show).toEqual(
      Show.Standard
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
  });

  test("updateDoc outputs an UPDATE: message if showOutput is true", async () => {
    await db.put({ _id: "name", foo: "bar" });
    await updateDoc({
      db,
      id: "name",
      payload: { foo2: "abc2" },
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
  });

  test("updateDoc outputs a NODIFF: message if showing output and no update needed", async () => {
    await db.put({ _id: "name", foo: "bar" });
    await updateDoc({
      db,
      id: "name",
      payload: { foo2: "abc2" },
      updateStrategy: "useOld",
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
  });

  test("updateDoc outputs a RENAME: UPDATE:", async () => {
    await db.put({
      _id: "docId",
      foo: "abc",
    });
    await updateDoc({
      db,
      id: "docId",
      payload: { _id: "newId", foo: "bar" },
      updateStrategy: "preferNew",
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("RENAME"));
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
  });

  test("updateDoc throws and outputs an EXISTS: FAILED:", async () => {
    await db.put({
      _id: "docId",
      foo: "abc",
    });
    await db.put({ _id: "conflictId", some: "data" });
    try {
      await updateDoc({
        db,
        id: "docId",
        payload: { _id: "conflictId", foo: "bar" },
        updateStrategy: "preferNew",
        outputArgs: {
          show: Show.Standard,
        },
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
  });

  test("overwriteDoc outputs OWRITE", async () => {
    await db.put({
      _id: "docId",
      data: { foo: "abc" },
      meta: { humanId: "abcd" },
    });
    await overwriteDoc({
      db,
      id: "docId",
      payload: {
        _id: "docId",
        data: { bar: "def" },
        meta: { humanId: "defg" },
      },
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("RENAME")
    );
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("UPDATE")
    );
  });

  test("overwriteDoc outputs a RENAME: OWRITE:", async () => {
    await db.put({
      _id: "docId",
      data: { foo: "abc" },
      meta: { humanId: "abcd" },
    });
    await overwriteDoc({
      db,
      id: "docId",
      payload: {
        _id: "newId",
        data: { bar: "def" },
        meta: { humanId: "defg" },
      },
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("OWRITE"));
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("RENAME"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("UPDATE")
    );
  });

  test("overwriteDoc outputs NODIFF", async () => {
    await db.put({
      _id: "docId",
      data: { foo: "abc" },
      meta: { humanId: "abcd" },
    });
    await overwriteDoc({
      db,
      id: "docId",
      payload: {
        _id: "docId",
        data: { foo: "abc" },
        meta: { humanId: "abcd" },
      },
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("OWRITE")
    );
  });

  test("overwriteDoc throws and outputs an EXISTS: FAILED:", async () => {
    await db.put({
      _id: "docId",
      data: { foo: "abc" },
      meta: { humanId: "abcd" },
    });
    await db.put({
      _id: "conflictId",
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
        outputArgs: {
          show: Show.Standard,
        },
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("OWRITE")
    );
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("RENAME")
    );
  });

  test("deleteDoc outputs DELETE", async () => {
    await db.put({ _id: "doc-to-delete" });
    await deleteDoc({
      db,
      id: "doc-to-delete",
      outputArgs: {
        show: Show.Standard,
      },
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("DELETE"));
  });

  test("show is None by default when calling a command via import or API", async () => {
    const spy = jest.spyOn(addDocModule, "addDoc").mockReturnValue(
      Promise.resolve({
        _id: "returnDoc",
        _rev: "1-abcd",
        data: {},
        meta: {},
      })
    );
    await addCmd({ idPart: "returnDoc" });
    expect(spy.mock.calls[0][0].outputArgs?.show).toBeUndefined();
  });

  // This test breaks because yargs can't seem to handle the jest environment now even when just parsing strings
  // test.skip("show is Standard by default when calling from the CLI", async () => {
  //   const spy = jest.spyOn(addDocModule, "addDoc").mockReturnValue(
  //     Promise.resolve({
  //       _id: "returnDoc",
  //       _rev: "1-abcd",
  //       data: {},
  //       meta: {},
  //     })
  //   );
  //
  //   await main(["--db", dbName, "add"]);
  //   expect(spy).toHaveBeenCalled();
  //   expect(spy.mock.calls[0][0].show).toEqual(Show.Standard);
  // });
});

const main = require("../src/index");
const nano = require("nano")("http://admin:password@localhost:5983");
const pass = () => {};
const fail = () => {
  throw Error;
};
const originalLog = console.log;

describe("main", () => {
  const mockedLog = jest.fn();

  beforeAll(async () => {
    await nano.db.destroy("datum").catch(pass);
  });

  beforeEach(async () => {
    await nano.db.create("datum").catch(pass);
    console.log = mockedLog;
  });

  afterEach(async () => {
    await nano.db.destroy("datum").catch(pass);
    console.log = originalLog;
    mockedLog.mockReset();
  });

  it("inserts documents into couchdb", async () => {
    await main({});

    const db = nano.use("datum");
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
  });

  it("creates the database if it doesn't exist", async () => {
    await nano.db.destroy("datum").catch(pass);

    await main({});
    nano.db.list().then((body) => {
      expect(body.includes("datum"));
    });
  });

  it("can undo adding documents with a known id", async () => {
    await main({ idPart: "this_one_should_be_deleted" });
    await main({ idPart: "kept" });

    const db = nano.use("datum");
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(2);
    });
    await db.get("this_one_should_be_deleted");
    await db.get("kept");

    mockedLog.mockReset();
    await main({ idPart: "this_one_should_be_deleted", undo: true });

    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("DELETE"));
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
    await db.get("kept");
    await expect(db.get("this_one_should_be_deleted")).rejects.toThrowError(
      "deleted"
    );
  });

  it("Can remove metadata entirely", async () => {
    expect(await main({ idPart: "hasMetadata" })).toHaveProperty("meta");
    expect(
      await main({ idPart: "noMeta", noMetadata: true })
    ).not.toHaveProperty("meta");
  });

  it("tells the user if the document already exists", async () => {
    await main({ idPart: "my name is bob" });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS")
    );

    mockedLog.mockReset();

    await main({ idPart: "my name is bob" });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("CREATE")
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  });

  it("inserts id structure into the metadata", async () => {
    expect(
      await main({ idPart: ["rawString", "%foo%!!"], _: ["foo=abc"] })
    ).toMatchObject({
      meta: { idStructure: "rawString__%foo%!!" },
    });
  });

  it("can use a custom payload as a base", async () => {
    expect(await main({ payload: '{a: 1, b:2, c:3 }', idPart: 'payload-doc1'})).toMatchObject({a:1, b:2, c:3, _id: "payload-doc1"})
  })

  it("throws a payload error if payload is malformed", async () => {
    expect(async () => await main({ payload: 'string'})).toThrowError
  })

  it("uses the payload _id if specified", async () => {
    expect(await main({ payload: '{ _id: payload-id }', idPart: 'argument-id'})).toMatchObject({_id: 'payload-id'})
    expect(await main({ payload: '{ _id: payload-id-2 }', idPart: '%keyId%', posArgs: ['keyId=key-id']})).toMatchObject({_id: 'payload-id-2'})
  })
});

const main = require("../src/index");
const nano = require("nano")("http://admin:password@localhost:5983");
const pass = () => {};

describe("main", () => {
  beforeAll(async () => {
    await nano.db.destroy("datum").catch(pass);
  });

  beforeEach(async () => {
    await nano.db.create("datum").catch(pass);
  });

  afterEach(async () => {
    await nano.db.destroy("datum").catch(pass);
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
    const db = nano.use("datum");
    await main({idField: "'this_one_should_be_deleted'"});
    await main({idField: "'kept"})

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(2);
    });
    await db.get("this_one_should_be_deleted")
    await db.get("kept")

    await main({idField: "'this_one_should_be_deleted'", undo: true})
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
    await db.get("kept")
    await expect(db.get("this_one_should_be_deleted")).rejects.toThrowError('deleted')
  })
});

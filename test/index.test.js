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
    await main();

    const db = nano.use("datum");
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
  });

  it("creates the database if it doesn't exist", async () => {
    await nano.db.destroy("datum").catch(pass);

    await main();
    nano.db.list().then((body) => {
      expect(body.includes("datum"));
    });
  });
});

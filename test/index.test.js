const main = require("../src/index");
const nano = require("nano")("http://admin:password@localhost:5983");

describe("main", () => {
  beforeAll( async () => {
    await nano.db.destroy('datum').catch((err) => undefined);
  });
  
  beforeEach( async () => {
    await nano.db.create('datum')
    .catch((err) => undefined);
  });

  afterEach( async () => {
    await nano.db.destroy('datum').catch(err => undefined);
  });

  it("inserts documents into couchdb", async () => {
    await main();

    const db = nano.use('datum');
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });

  });

  it("creates the database if it doesn't exist", async () => {
    await nano.db.destroy('datum').catch(() => {});
    
    await main();
    nano.db.list().then(body => {
      expect(body.includes ('datum'));
    });
  });
});

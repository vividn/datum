#!/usr/bin/env node

async function main() {
  
  const dbName = 'datum';
  
  const nano = require("nano")("http://admin:password@localhost:5983");
  
  // Create database if it doesn't exist
  await nano.db.create(dbName).catch((err: any) => undefined);
 
  const db = nano.use(dbName);

  const _id = new Date().toISOString();

  await db.insert({"_id": _id})
    .then(() => db.get(_id))
    .then((body: any) => console.log(body))
    .catch((err: any) => console.error(err));
}

if (require.main === module) {
  main();
}

module.exports = main;

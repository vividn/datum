#!/usr/bin/env node

function main() {
    console.log("Hello World!");
    const nano = require('nano')('http://localhost:5984');
    const db = nano.use("datum");

    const _id = new Date().toISOString();

    db.insert({}, _id).catch((err: any) => console.log(err))
}
main();

module.exports = main;

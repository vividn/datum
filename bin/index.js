#!/usr/bin/env node

db = require('nano')('http://localhost:5984/datum');

let argv = require('yargs')
    .command('datum', 'quickly insert timestamped data into couchdb')
    .help('h')
    .alias('h', 'help')

    // TODO
    .describe('d', 'specify date of the timestamp, use `+n` or `-n` for a date relative to today')
    .alias('d','date')

    // TODO
    .describe('t', "specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now")
    .alias('t', 'time')


    .argv

console.log(argv);

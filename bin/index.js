#!/usr/bin/env node

db = require('nano')('http://localhost:5984/datum');
chrono = require('chrono-node')
let argv = require('yargs')
    .command('datum', 'quickly insert timestamped data into couchdb')
    .help('h')
    .alias('h', 'help')

    .options({
        'f': {
            describe: 'the primary field of the data',
            alias: 'field',
            nargs: 1,
            demandOption: true
        },
        'd': {
            describe: 'specify date of the timestamp, use `+n` or `-n` for a date relative to today',
            alias: 'date',
            nargs: 1,
        },
        't': {
            describe: "specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now",
            alias: 'time',
            nargs: 1
        },
        "u": {
            describe: "undoes the last datum entry, can be combined with -f",
            alias: "undo",
            type: "boolean"
        },
        "U": {
            describe: "forces an undo, even if the datapoint was entered more than 15 minutes ago",
            alias: "force-undo",
            type: "boolean"
        },
        "A": {
            describe: "The keys to use for additional data, useful for aliases",
            alias: "additional-data",
            type: "array"
        },
        "a": {
            describe: "Terminate the -A array"
        }
    })

    // TODO
    .example("alias foobar='datum -f abc -A foo bar -a'; foobar 3 6","creates a document with the abc field {foo: 3, bar: 6}")

    .argv

console.log(chrono.parseDate('3pm'))

console.log(argv);

const payload = {};

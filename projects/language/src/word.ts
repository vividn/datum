#!/usr/bin/env node

import yargs from "yargs";
import sample from "lodash.sample";
import { homedir } from "os";
import { connectDb } from "../../../src/auth/connectDb";
import { ViewRow } from "../../../src/utils/utilityTypes";
import { startsWith } from "../../../src/utils/startsWith";
import promptSync from "prompt-sync";
import picocolors from "picocolors";
import { spawnSync } from "child_process";

async function main() {
  const args = await yargs
    .options({
      start: {
        alias: "s",
        type: "string",
        description:
          "start key to use. If used without --end, then interpreted as a startsWith string",
      },
      end: {
        alias: "e",
        type: "string",
        description: "end key to use",
      },
      view: {
        alias: "v",
        type: "string",
        description:
          "specify a view from which to sample. If unspecified will sample from _all_docs",
      },
    })
    .strict().argv;
  const { start, end, view } = args;

  args.env = homedir() + "/Projects/Datum/couchdb/prod.env";
  args.db = "language";
  args._ = [];

  const db = await connectDb(args);

  const filter = start
    ? end
      ? { start_key: start, end_key: end }
      : startsWith(start)
    : {};

  if (view !== undefined) {
    throw new Error("not implemented yet");
  }
  // const all_doc_ids = (await db.list({ ...filter })).rows.map((row) => row.id);
  const docs = await db.allDocs({ include_docs: true, ...filter });

  while (true) {
    console.clear();
    const rando = sample(docs.rows as any[]).doc;

    const humanId = rando.meta?.humanId;
    const hid = humanId ? `(${humanId.slice(0, 5)})` : "";
    const data = rando.data ?? rando;
    const text = data.article ? `${data.article} ${data.source}` : data.source;

    const prompt = promptSync({ sigint: true });
    prompt(
      `${picocolors.yellow(picocolors.bold(text))} (${data.srcLang}-${
        data.pos
      }) ${hid}`
    );
    const ipa = spawnSync("espeak", [
      "-q",
      "-v",
      data.srcLang,
      "--ipa",
      data.source,
    ])
      .stdout.toString()
      .trim();
    console.log(ipa);
    console.log("---------------");
    const english = data.en;
    if (Array.isArray(english)) {
      console.log(english.join("\n"));
    } else if (english) {
      console.log(english);
    } else {
      console.log("<english missing>");
    }
    prompt("");
  }
}

if (require.main === module) {
  // Load command line arguments
  main();
}

export {};

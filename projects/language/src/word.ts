#!/usr/bin/env node

import sample from "lodash.sample";
import { homedir } from "os";
import { connectDb } from "../../../src/auth/connectDb";
import { startsWith } from "../../../src/utils/startsWith";
import promptSync from "prompt-sync";
import picocolors from "picocolors";
import { spawnSync } from "child_process";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../../../src/utils/parseIfNeeded";
import { MainDatumArgs } from "../../../src/input/mainArgs";

export const wordCommand = new ArgumentParser({
  description: "flash card system for word review",
  add_help: true,
  prog: "word",
  usage: "word [-s START] [-e END]",
});
wordCommand.add_argument("-s", "--start", {
  type: "str",
  help: "start key to use. If used without --end, then interpreted as a startsWith string",
});
wordCommand.add_argument("-e", "--end", {
  type: "str",
  help: "end key to use",
});
wordCommand.add_argument("-v", "--view", {
  type: "str",
  help: "specify a view from which to sample. If unspecified will sample from _all_docs",
});
wordCommand.set_defaults({ db: "language" });

export type WordArgs = {
  start?: string;
  end?: string;
  view?: string;
} & MainDatumArgs;

async function word(argsOrCli: WordArgs | string | string[]): Promise<void> {
  const args = parseIfNeeded(wordCommand, argsOrCli);
  const { start, end, view } = args;

  const db = connectDb(args);

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

  // eslint-disable-next-line no-constant-condition
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
      }) ${hid}`,
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
  word(process.argv.slice(2));
}

export {};

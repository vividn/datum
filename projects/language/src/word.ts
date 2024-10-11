#!/usr/bin/env node

import readline from "node:readline";
import { stdin, stdout } from "node:process";
import { once } from "node:events";
import sample from "lodash.sample";
import { connectDb } from "../../../src/auth/connectDb";
import { startsWith } from "../../../src/utils/startsWith";
import picocolors from "picocolors";
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
wordCommand.add_argument("-t", "--time", {
  type: "int",
  help: "starts slideshow mode, just displaying words and translations every n seconds",
});
wordCommand.set_defaults({ db: "language" });

export type WordArgs = {
  start?: string;
  end?: string;
  view?: string;
  time?: number;
} & MainDatumArgs;

const FLAGS: Record<string, string> = {
  en: "🇺🇸",
  es: "🇪🇸",
  fr: "🇫🇷",
  de: "🇩🇪",
  it: "🇮🇹",
  pt: "🇵🇹",
  ru: "🇷🇺",
  nl: "🇳🇱",
  da: "🇩🇰",
  sv: "🇸🇪",
} as const;

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

  let header: string, srcWord: string, translation: string;
  function newWord() {
    const rando = sample(docs.rows as any[]).doc;

    const humanId = rando.meta?.humanId;
    const hid = humanId ? `(${humanId.slice(0, 5)})` : "";
    const data = rando.data ?? rando;
    const text = data.article ? `${data.article} ${data.source}` : data.source;
    const english: string[] | undefined = Array.isArray(data.en)
      ? data.en
      : data.en
        ? [data.en]
        : undefined;
    const bulletedEnglish = english?.map((en) => `• ${en}`).join("\n");

    header = `${FLAGS[data.srcLang] ?? data.srcLang} ${data.pos} ${picocolors.dim(hid)}`;
    srcWord = picocolors.yellow(picocolors.bold(text));
    translation = bulletedEnglish ?? " <english missing>";
  }

  let inPrompt = false;
  function promptResponse() {
    if (!inPrompt) {
      newWord();
      console.clear();
      console.log(header);
      console.log(srcWord);
      inPrompt = true;
    } else {
      console.log(translation);
      inPrompt = false;
    }
  }

  function noPrompt() {
    newWord();
    console.clear();
    console.log(header);
    console.log(srcWord);
    console.log(translation);
  }

  const interval = args.time
    ? setInterval(noPrompt, args.time * 1000)
    : undefined;

  const rl = readline.createInterface({
    input: stdin,
    output: undefined,
    terminal: true,
  });
  stdout.write("\u001B[?25l"); // hides cursor
  stdin.setRawMode(true);

  rl.on("line", async () => {
    promptResponse();
  });
  rl.on("close", () => {
    interval && clearInterval(interval);
    stdout.write("\u001B[?25h"); // show cursor
    stdin.setRawMode(false);
  });
  rl.on("SIGINT", () => {
    interval && clearInterval(interval);
    rl.close();
    process.exit(0);
  });

  if (args.time) {
    noPrompt();
  } else {
    promptResponse();
  }

  await once(rl, "close");
  return;
}

if (require.main === module) {
  // Load command line arguments
  word(process.argv.slice(2));
}

export {};

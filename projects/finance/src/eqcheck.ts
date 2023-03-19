#!/usr/bin/env node

import { BaseArgs, baseArgs } from "../../../src/input/baseArgs";
import { connectDb } from "../../../src/auth/connectDb";
import { viewMap } from "../../../src/views/viewMap";
import { equalityView } from "../views";

async function main(cliInput: string | string[]) {
  const args = (await baseArgs.parse(cliInput)) as BaseArgs;
  args.db ??= "finance";
  const db = connectDb(args);
  const allEqualityChecks = (await viewMap({ db, datumView: equalityView })).rows;

  console.log({allEqualityChecks});
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}

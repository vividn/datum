#!/usr/bin/env node
import { configuredYargs } from "./input";
import { DocExistsError } from "./documentControl/base";

async function main(cliInput: string | string[]): Promise<void> {
  await configuredYargs.parseAsync(cliInput);
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    if (err instanceof DocExistsError) {
      process.exitCode = 11;
    } else {
      console.error(err);
    }
  });
}

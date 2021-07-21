#!/usr/bin/env node
import { configuredYargs } from "./input";

async function main(cliInput: string | string[]): Promise<void> {
  await configuredYargs.parseAsync(cliInput);
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}

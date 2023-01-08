#!/usr/bin/env node

async function main(_cliInput: string | string[]) {
  console.log("finance is working");
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}

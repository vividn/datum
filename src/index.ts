#!/usr/bin/env node
import { DocExistsError } from "./documentControl/base";
import { datum } from "./input/mainArgs";

if (require.main === module) {
  if (["", undefined].includes(process.env["NODE_ENV"])) {
    process.env.NODE_ENV = "production";
  }
  datum(process.argv.slice(2)).catch((err) => {
    if (err instanceof DocExistsError) {
      process.exit(11);
    } else {
      if (process.env["NODE_ENV"] === "production") {
        console.error(`${err.name ?? "Error"}:`, err.message);
      } else {
        console.error(err);
      }
      process.exit(1);
    }
  });
}

export { Datum } from "./Datum";

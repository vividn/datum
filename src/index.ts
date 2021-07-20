#!/usr/bin/env node
import { BaseDatumArgs, configuredYargs } from "./input";
import chalk from "chalk";
import { BaseDataError } from "./errors";
import dotenv from "dotenv";
import Nano, { DocumentScope } from "nano";
import { parseData } from "./parseData";
import inferType from "./utils/inferType";
import { processTimeArgs } from "./timings";
import { assembleId, buildIdStructure, defaultIdComponents } from "./ids";
import pass from "./utils/pass";
import {
  DataOnlyPayload,
  DatumMetadata,
  DatumPayload,
  EitherDocument,
  EitherPayload,
} from "./documentControl/DatumDocument";
import newHumanId from "./meta/newHumanId";
import { defaults } from "./input/defaults";
import { showCreate, showExists } from "./output";
import addDoc from "./documentControl/addDoc";
import addCmd, { AddCmdArgs } from "./commands/addCmd";
import { connectDb } from "./auth";

if (require.main === module) {
  // Load command line arguments
  const args = configuredYargs.parse(process.argv.slice(2)) as
    | BaseDatumArgs
    | AddCmdArgs;
  if (args._[0] === "add") {
    addCmd(args as AddCmdArgs).then((doc) => {
      console.log(doc);
    }).catch((err) => {
      console.error(err);
    });
  } else {
    console.log(args);
  }
}

import { MainDatumArgs } from "../../input/mainArgs";
import { DatumConfig } from "../loadConfig";
import os from "os";

const tempDir = process.env["RUNNER_TEMP"] ?? os.tmpdir();
export function loadConfig(_args: MainDatumArgs): DatumConfig {
  return {
    project_dir: tempDir,
    db: "test",
    connection: {
      host: "%MEMORY%",
      user: "user",
      password: "password",
    },
  };
}

import { MainDatumArgs } from "../../input/mainArgs.js";
import { DatumConfig } from "../loadConfig.js";

export function loadConfig(_args: MainDatumArgs): DatumConfig {
  return {
    db: "test",
    host: "%MEMORY%",
    user: "user",
    password: "password",
  };
}

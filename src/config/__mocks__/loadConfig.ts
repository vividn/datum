import { MainDatumArgs } from "../../input/mainArgs";
import { DatumConfig } from "../loadConfig";

export function loadConfig(_args: MainDatumArgs): DatumConfig {
  return {
    db: "test",
    host: "%MEMORY%",
    user: "user",
    password: "password",
  };
}

import { loadConfig } from "../loadConfig";
import * as initConfigModule from "../initConfig";
import fs from "fs";

const configFile = `${__dirname}/__fixtures__/test_datumrc.yml`;

jest.unmock("../loadConfig");
describe("loadConfig", () => {
  it("should load a config file", () => {
    const args = {
      configFile,
    };
    const config = loadConfig(args);
    expect(config).toEqual({
      db: "test_db",
      host: process.env["HOME"] + "/.local/share/test_host",
      user: "test_user",
      password: "test_password",
    });
  });

  it("should throw an error if the config file doesn't exist", () => {
    const initConfigSpy = jest
      .spyOn(initConfigModule, "initConfig")
      .mockReturnValue({ db: "default_db" });
    const args = {
      configFile: "/test/fixtures/nonexistent.yml",
    };
    expect(() => loadConfig(args)).toThrow(
      "Config file not found: /test/fixtures/nonexistent.yml",
    );
    expect(initConfigSpy).not.toHaveBeenCalled();
  });

  it("write a default config if no config path is specified and no config file exists at the default location, creating parent folders as needed", () => {
    const initConfigSpy = jest
      .spyOn(initConfigModule, "initConfig")
      .mockReturnValue({ db: "default_db" });
    jest.spyOn(fs, "readFileSync").mockImplementation(() => {
      throw { code: "ENOENT" };
    });
    const config = loadConfig({});
    expect(initConfigSpy).toHaveBeenCalled();
    expect(config).toEqual({ db: "default_db" });
  });

  it("should replace environment variables in the config", () => {
    process.env["HOME"] = "/home/test";
    process.env["XDG_DATA_HOME"] = "/home/test/.local/share";
    const args = {
      configFile,
    };
    const config = loadConfig(args);
    expect(config).toEqual({
      db: "test_db",
      host: "/home/test/.local/share/test_host",
      user: "test_user",
      password: "test_password",
    });
  });
});

import fs from "fs";
import { initCmd } from "../initCmd";
import { pass } from "../../__test__/test-utils";

const defaultConfigFile = fs.readFileSync(
  `${__dirname}/../../config/defaultConfig.yml`,
  "utf-8",
);

describe("initCmd", () => {
  beforeEach(() => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    try {
      fs.rmSync("/tmp/datum/datumrc.yml");
    } catch (e) {
      pass;
    }
  });
  it("writes datumrc into the xdg_config directory", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    await initCmd({ nonInteractive: true });
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    expect(configFile).toEqual(defaultConfigFile);
  });

  it("fails if datumrc exists and --overwrite is not specified", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    fs.writeFileSync("/tmp/datum/datumrc.yml", "test");
    await expect(initCmd({ nonInteractive: true })).rejects.toThrow();
  });

  it("overwrites datumrc if it exists and --overwrite is specified", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    fs.writeFileSync("/tmp/datum/datumrc.yml", "test");
    await initCmd({ nonInteractive: true, overwrite: true });
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    expect(configFile).toEqual(defaultConfigFile);
  });
});
